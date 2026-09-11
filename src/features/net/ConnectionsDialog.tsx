// ============================================================
// Manage remote connections (SFTP, ticket 021), the credential backend
// behind them (ticket 027) and the SSH host-key trust for them
// (ticket 028).
//
// Panels inside one dialog: the list of saved profiles (connect /
// disconnect / edit / delete), the add/edit form, the credential-store
// settings (OS keychain ↔ KeePass file), the KeePass unlock prompt, the
// host-key confirmation and the explicit "drop stored host keys" step.
// Secrets are entered here but sent straight to the backend; neither
// form keeps them around after submitting.
// ============================================================

import { useEffect, useState } from "react";
import {
  KeyRound,
  Lock,
  LockOpen,
  Pencil,
  Plug,
  PlugZap,
  Plus,
  Server,
  ShieldAlert,
  ShieldCheck,
  ShieldOff,
  Trash2,
} from "lucide-react";
import type { CredConfig, Profile } from "@/ipc/bindings";
import { useNet } from "@/features/net/netStore";
import { usePanes } from "@/store/panesStore";
import { useT } from "@/store/settingsStore";
import { AppDialog } from "@/components/ui/AppDialog";
import { cn } from "@/lib/cn";

/** Supported protocols with their default port. */
const PROTOCOLS = ["sftp", "ftp", "ftps", "webdav", "smb"] as const;
type ProtocolId = (typeof PROTOCOLS)[number];

const DEFAULT_PORT: Record<ProtocolId, number> = {
  sftp: 22,
  ftp: 21,
  ftps: 21,
  webdav: 443,
  smb: 445,
};

/**
 * Auth methods a protocol actually supports — ssh-agent/keys are
 * SSH-only, Digest is WebDAV-only.
 */
type AuthId = "password" | "agent" | "key" | "anonymous" | "digest";

const AUTH_FOR: Record<ProtocolId, readonly AuthId[]> = {
  sftp: ["password", "agent", "key", "anonymous"],
  ftp: ["password", "anonymous"],
  ftps: ["password", "anonymous"],
  webdav: ["password", "digest", "anonymous"],
  smb: ["password", "anonymous"],
};

function emptyProfile(): Profile {
  return {
    id: crypto.randomUUID(),
    name: "",
    protocol: "sftp",
    host: "",
    port: DEFAULT_PORT.sftp,
    user: "",
    auth: "password",
    key_path: null,
    start_path: "/",
  };
}

export function ConnectionsDialog() {
  const open = useNet((s) => s.managerOpen);
  const close = useNet((s) => s.closeManager);
  const connections = useNet((s) => s.connections);
  const profiles = useNet((s) => s.profiles);
  const editing = useNet((s) => s.editing);
  const error = useNet((s) => s.error);
  const panel = useNet((s) => s.panel);
  const startEdit = useNet((s) => s.startEdit);
  const showPanel = useNet((s) => s.showPanel);
  const t = useT();

  return (
    <AppDialog
      dialogKey="connections"
      open={open}
      onClose={close}
      titleBar={
        <>
          <Server size={15} className="text-accent" />
          {t("net.title")}
        </>
      }
      defaultSize={{ w: 560, h: 460 }}
      minSize={{ w: 440, h: 340 }}
    >
      <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
        {error && (
          <div className="rounded border border-red-500/40 bg-red-500/10 px-3 py-1.5 text-[12px] text-red-400">
            {error}
          </div>
        )}
        {editing ? (
          <ProfileForm profile={editing} />
        ) : panel === "hostkey" ? (
          <HostKeyPanel />
        ) : panel === "forget" ? (
          <ForgetHostKeyPanel />
        ) : panel === "store" ? (
          <CredentialStoreForm />
        ) : panel === "unlock" ? (
          <UnlockForm />
        ) : (
          <>
            <div className="flex min-h-0 flex-1 flex-col gap-1 overflow-auto">
              {profiles.length === 0 && (
                <p className="px-1 py-4 text-center text-[12px] text-dim">
                  {t("net.empty")}
                </p>
              )}
              {profiles.map((p) => {
                const status = connections.find((c) => c.id === p.id);
                return (
                  <ConnectionRow
                    key={p.id}
                    profile={p}
                    connected={!!status?.connected}
                  />
                );
              })}
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => startEdit(emptyProfile())}
                className="flex items-center gap-1.5 rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
              >
                <Plus size={14} />
                {t("net.add")}
              </button>
              <button
                onClick={() => showPanel("store")}
                title={t("cred.title")}
                className="ml-auto flex items-center gap-1.5 rounded border border-edge bg-panel px-3 py-1 text-[13px] text-dim hover:border-accent hover:text-text"
              >
                <KeyRound size={14} />
                {t("cred.manage")}
              </button>
            </div>
          </>
        )}
      </div>
    </AppDialog>
  );
}

function ConnectionRow({
  profile,
  connected,
}: {
  profile: Profile;
  connected: boolean;
}) {
  const connect = useNet((s) => s.connect);
  const disconnect = useNet((s) => s.disconnect);
  const remove = useNet((s) => s.remove);
  const startEdit = useNet((s) => s.startEdit);
  const startForget = useNet((s) => s.startForget);
  const closeManager = useNet((s) => s.closeManager);
  const t = useT();

  const doConnect = async () => {
    const address = await connect(profile.id);
    if (address) {
      const s = usePanes.getState();
      closeManager();
      void s.loadDir(s.active, address);
    }
  };

  return (
    <div className="flex items-center gap-2 rounded border border-edge bg-panel-inactive px-2.5 py-1.5">
      <span
        className={cn(
          "h-2 w-2 shrink-0 rounded-full",
          connected ? "bg-emerald-400" : "bg-dim/50",
        )}
        title={connected ? t("net.connected") : ""}
      />
      <div className="flex min-w-0 flex-col leading-tight">
        <span className="truncate text-[13px] text-text">
          {profile.name || profile.host}
        </span>
        <span className="truncate font-mono text-[10px] text-dim">
          {profile.protocol}://{profile.user}@{profile.host}:{profile.port}
        </span>
      </div>
      <div className="ml-auto flex shrink-0 items-center gap-1">
        {connected ? (
          <button
            onClick={() => void disconnect(profile.id)}
            title={t("net.disconnect")}
            className="rounded border border-edge p-1 text-dim hover:border-accent hover:text-text"
          >
            <Plug size={14} />
          </button>
        ) : (
          <button
            onClick={() => void doConnect()}
            title={t("net.connect")}
            className="rounded border border-edge p-1 text-emerald-400 hover:border-accent"
          >
            <PlugZap size={14} />
          </button>
        )}
        {profile.protocol === "sftp" && (
          <button
            onClick={() => startForget(profile)}
            title={t("hostkey.forget")}
            className="rounded border border-edge p-1 text-dim hover:border-accent hover:text-text"
          >
            <ShieldOff size={14} />
          </button>
        )}
        <button
          onClick={() => startEdit(profile)}
          title={t("net.edit")}
          className="rounded border border-edge p-1 text-dim hover:border-accent hover:text-text"
        >
          <Pencil size={14} />
        </button>
        <button
          onClick={() => void remove(profile.id)}
          title={t("net.delete")}
          className="rounded border border-edge p-1 text-dim hover:border-red-500 hover:text-red-400"
        >
          <Trash2 size={14} />
        </button>
      </div>
    </div>
  );
}

function ProfileForm({ profile }: { profile: Profile }) {
  const save = useNet((s) => s.save);
  const cancelEdit = useNet((s) => s.cancelEdit);
  const t = useT();

  const [form, setForm] = useState<Profile>(profile);
  const [secret, setSecret] = useState("");

  // Reset when a different profile is opened for editing.
  useEffect(() => {
    setForm(profile);
    setSecret("");
  }, [profile]);

  const set = <K extends keyof Profile>(key: K, value: Profile[K]) =>
    setForm((f) => ({ ...f, [key]: value }));

  /** Switching protocol adjusts the port and keeps auth valid. */
  const setProtocol = (proto: ProtocolId) =>
    setForm((f) => {
      const allowed = AUTH_FOR[proto];
      return {
        ...f,
        protocol: proto,
        // Only replace a port the user has not customised.
        port: DEFAULT_PORT[(f.protocol as ProtocolId) ?? "sftp"] === f.port
          ? DEFAULT_PORT[proto]
          : f.port,
        auth: allowed.includes(f.auth as AuthId) ? f.auth : allowed[0],
      };
    });

  const proto = (form.protocol as ProtocolId) ?? "sftp";
  const auth = form.auth;
  const authMethods = AUTH_FOR[proto] ?? AUTH_FOR.sftp;
  const canSave = form.host.trim() !== "" && form.port > 0;

  const submit = () => {
    if (!canSave) return;
    const name = form.name.trim() || form.host.trim();
    const secretToSave = secret.trim() === "" ? null : secret.trim();
    void save({ ...form, name }, secretToSave);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-auto">
      <Field label={t("net.protocol")}>
        <select
          value={proto}
          onChange={(e) => setProtocol(e.target.value as ProtocolId)}
          className={inputCls}
        >
          {PROTOCOLS.map((pr) => (
            <option key={pr} value={pr}>
              {t(`net.proto.${pr}` as const)}
            </option>
          ))}
        </select>
      </Field>

      {/* Plain FTP sends credentials in the clear — say so (ticket 025). */}
      {proto === "ftp" && (
        <div className="flex items-start gap-2 rounded border border-amber-500/40 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-400">
          <ShieldAlert size={14} className="mt-px shrink-0" />
          <span>{t("net.ftpWarning")}</span>
        </div>
      )}

      <Field label={t("net.name")}>
        <input
          value={form.name}
          onChange={(e) => set("name", e.target.value)}
          className={inputCls}
          autoComplete="off"
          spellCheck={false}
        />
      </Field>
      <div className="flex gap-2">
        <Field label={t("net.host")} className="flex-1">
          <input
            value={form.host}
            onChange={(e) => set("host", e.target.value)}
            className={inputCls}
            autoComplete="off"
            spellCheck={false}
          />
        </Field>
        <Field label={t("net.port")} className="w-24">
          <input
            type="number"
            value={form.port}
            onChange={(e) => set("port", Number(e.target.value) || 0)}
            className={inputCls}
          />
        </Field>
      </div>
      <Field label={t("net.user")}>
        <input
          value={form.user}
          onChange={(e) => set("user", e.target.value)}
          className={inputCls}
          autoComplete="off"
          spellCheck={false}
        />
        {proto === "smb" && (
          <span className="text-[10px] text-dim">{t("net.domainHint")}</span>
        )}
      </Field>
      <Field label={t("net.auth")}>
        <select
          value={form.auth}
          onChange={(e) => set("auth", e.target.value)}
          className={inputCls}
        >
          {authMethods.map((m) => (
            <option key={m} value={m}>
              {t(`net.auth.${m}` as const)}
            </option>
          ))}
        </select>
      </Field>
      {(auth === "password" || auth === "digest") && (
        <Field label={t("net.password")}>
          <input
            type="password"
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            className={inputCls}
            autoComplete="off"
          />
        </Field>
      )}
      {auth === "key" && (
        <>
          <Field label={t("net.keyPath")}>
            <input
              value={form.key_path ?? ""}
              onChange={(e) => set("key_path", e.target.value || null)}
              className={inputCls}
              autoComplete="off"
              spellCheck={false}
            />
          </Field>
          <Field label={t("net.passphrase")}>
            <input
              type="password"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              className={inputCls}
              autoComplete="off"
            />
          </Field>
        </>
      )}
      <Field label={t("net.startPath")}>
        <input
          value={form.start_path}
          onChange={(e) => set("start_path", e.target.value)}
          className={inputCls}
          autoComplete="off"
          spellCheck={false}
        />
      </Field>

      <div className="mt-1 flex justify-end gap-2">
        <button
          onClick={cancelEdit}
          className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
        >
          {t("net.cancel")}
        </button>
        <button
          onClick={submit}
          disabled={!canSave}
          className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
        >
          {t("net.save")}
        </button>
      </div>
    </div>
  );
}

/**
 * Credential-store settings: OS keychain (default) ↔ KeePass file.
 * The master password is never entered here — that is the unlock
 * prompt's job, so nothing typed on this panel is a secret.
 */
function CredentialStoreForm() {
  const cred = useNet((s) => s.cred);
  const setCredBackend = useNet((s) => s.setCredBackend);
  const showPanel = useNet((s) => s.showPanel);
  const credLock = useNet((s) => s.credLock);
  const t = useT();

  const [form, setForm] = useState<CredConfig>(
    cred?.config ?? {
      backend: "keychain",
      kdbx_path: "",
      key_file: "",
      master_in_keychain: false,
    },
  );

  // Adopt the backend's own answer once it arrives / after a switch.
  useEffect(() => {
    if (cred) setForm(cred.config);
  }, [cred]);

  const set = <K extends keyof CredConfig>(key: K, value: CredConfig[K]) =>
    setForm((f) => ({ ...f, [key]: value }));

  const keepass = form.backend === "keepass";
  const canApply = !keepass || form.kdbx_path.trim() !== "";

  const apply = async () => {
    if (!canApply) return;
    const ok = await setCredBackend(form);
    // Switching to KeePass leaves a locked file — ask for the master
    // password right away instead of waiting for the next connect.
    if (ok && keepass) showPanel("unlock");
    else if (ok) showPanel("list");
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-auto">
      <Field label={t("cred.backend")}>
        <select
          value={form.backend}
          onChange={(e) => set("backend", e.target.value)}
          className={inputCls}
        >
          <option value="keychain">{t("cred.backend.keychain")}</option>
          <option value="keepass">{t("cred.backend.keepass")}</option>
        </select>
      </Field>

      {keepass && (
        <>
          <p className="rounded border border-edge bg-panel-inactive px-2.5 py-1.5 text-[11px] text-dim">
            {t("cred.keepassHint")}
          </p>
          <Field label={t("cred.kdbxPath")}>
            <input
              value={form.kdbx_path}
              onChange={(e) => set("kdbx_path", e.target.value)}
              placeholder="~/Documents/passwords.kdbx"
              className={inputCls}
              autoComplete="off"
              spellCheck={false}
            />
          </Field>
          <Field label={t("cred.keyFile")}>
            <input
              value={form.key_file}
              onChange={(e) => set("key_file", e.target.value)}
              className={inputCls}
              autoComplete="off"
              spellCheck={false}
            />
          </Field>
          <label className="flex items-start gap-2 text-[12px] text-text">
            <input
              type="checkbox"
              checked={form.master_in_keychain}
              onChange={(e) => set("master_in_keychain", e.target.checked)}
              className="mt-0.5"
            />
            <span className="flex flex-col gap-0.5">
              {t("cred.masterInKeychain")}
              <span className="text-[11px] text-dim">{t("cred.masterHint")}</span>
            </span>
          </label>
          {cred?.config.backend === "keepass" && (
            <div className="flex items-center gap-2 text-[12px]">
              {cred.unlocked ? (
                <>
                  <LockOpen size={14} className="text-emerald-400" />
                  <span className="text-dim">{t("cred.state.unlocked")}</span>
                  <button
                    onClick={() => void credLock()}
                    className="rounded border border-edge px-2 py-0.5 text-dim hover:border-accent hover:text-text"
                  >
                    {t("cred.lock")}
                  </button>
                </>
              ) : (
                <>
                  <Lock size={14} className="text-dim" />
                  <span className="text-dim">{t("cred.state.locked")}</span>
                  <button
                    onClick={() => showPanel("unlock")}
                    className="rounded border border-edge px-2 py-0.5 text-dim hover:border-accent hover:text-text"
                  >
                    {t("cred.unlock")}
                  </button>
                </>
              )}
            </div>
          )}
        </>
      )}

      <div className="mt-1 flex justify-end gap-2">
        <button
          onClick={() => showPanel("list")}
          className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
        >
          {t("cred.back")}
        </button>
        <button
          onClick={() => void apply()}
          disabled={!canApply}
          className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
        >
          {t("cred.apply")}
        </button>
      </div>
    </div>
  );
}

/**
 * Asks for the KeePass master password. Shown on demand and whenever an
 * action ran into the locked file — `credUnlock` then replays it, so the
 * click the user made still happens.
 */
function UnlockForm() {
  const cred = useNet((s) => s.cred);
  const credUnlock = useNet((s) => s.credUnlock);
  const showPanel = useNet((s) => s.showPanel);
  const t = useT();

  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(true);
  const [create, setCreate] = useState(false);
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (busy) return;
    setBusy(true);
    // The password leaves this component with the call and is not kept.
    const entered = password;
    setPassword("");
    await credUnlock(entered, remember, create);
    setBusy(false);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-auto">
      <div className="flex items-center gap-2 text-[13px] text-text">
        <Lock size={14} className="text-accent" />
        {t("cred.unlockTitle")}
      </div>
      <p className="truncate font-mono text-[11px] text-dim">
        {cred?.config.kdbx_path}
      </p>
      <Field label={t("cred.master")}>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void submit();
          }}
          className={inputCls}
          autoComplete="off"
          autoFocus
        />
      </Field>
      <label className="flex items-center gap-2 text-[12px] text-text">
        <input
          type="checkbox"
          checked={remember}
          onChange={(e) => setRemember(e.target.checked)}
        />
        {t("cred.remember")}
      </label>
      <label className="flex items-center gap-2 text-[12px] text-text">
        <input
          type="checkbox"
          checked={create}
          onChange={(e) => setCreate(e.target.checked)}
        />
        {t("cred.create")}
      </label>
      <div className="mt-1 flex justify-end gap-2">
        <button
          onClick={() => showPanel("store")}
          className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
        >
          {t("cred.cancel")}
        </button>
        <button
          onClick={() => void submit()}
          disabled={busy}
          className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
        >
          {t("cred.unlock")}
        </button>
      </div>
    </div>
  );
}

/**
 * The host-key question raised by a connect attempt (ticket 028). On
 * first contact the user confirms the fingerprint; a *changed* key is a
 * dead end here on purpose — dropping the stored key is a separate,
 * deliberate step, so no single click can wave a changed key through.
 */
function HostKeyPanel() {
  const challenge = useNet((s) => s.hostKey);
  const trust = useNet((s) => s.trustHostKey);
  const dismiss = useNet((s) => s.dismissHostKey);
  const closeManager = useNet((s) => s.closeManager);
  const t = useT();
  const [busy, setBusy] = useState(false);

  if (!challenge) return null;
  const changed = challenge.status === "mismatch";

  const accept = async () => {
    if (busy) return;
    setBusy(true);
    const address = await trust();
    setBusy(false);
    if (address) {
      const s = usePanes.getState();
      closeManager();
      void s.loadDir(s.active, address);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-auto">
      <div className="flex items-center gap-2 text-[13px] text-text">
        {changed ? (
          <ShieldAlert size={15} className="text-red-400" />
        ) : (
          <ShieldCheck size={15} className="text-accent" />
        )}
        {changed ? t("hostkey.changedTitle") : t("hostkey.unknownTitle")}
      </div>

      <p
        className={cn(
          "rounded border px-2.5 py-1.5 text-[11px]",
          changed
            ? "border-red-500/40 bg-red-500/10 text-red-400"
            : "border-edge bg-panel-inactive text-dim",
        )}
      >
        {changed ? t("hostkey.changedHint") : t("hostkey.unknownHint")}
      </p>

      <Field label={t("hostkey.server")}>
        <p className={cn(inputCls, "truncate")}>
          {challenge.host}:{challenge.port}
        </p>
      </Field>
      <Field
        label={changed ? t("hostkey.presented") : t("hostkey.fingerprint")}
      >
        <p className={cn(inputCls, "break-all")}>
          {challenge.key_type} {challenge.fingerprint}
        </p>
      </Field>

      {challenge.stored.length > 0 && (
        <Field label={t("hostkey.stored")}>
          <div className="flex flex-col gap-1">
            {challenge.stored.map((entry) => (
              <p
                key={`${entry.source}:${entry.line}`}
                className={cn(inputCls, "break-all")}
              >
                {entry.key_type} {entry.fingerprint}
                <span className="block text-[10px] text-dim">
                  {entry.source}:{entry.line}
                </span>
              </p>
            ))}
          </div>
        </Field>
      )}

      <div className="mt-1 flex justify-end gap-2">
        <button
          onClick={dismiss}
          className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
        >
          {changed ? t("hostkey.close") : t("net.cancel")}
        </button>
        {!changed && (
          <button
            onClick={() => void accept()}
            disabled={busy || !challenge.token}
            className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
          >
            {t("hostkey.trust")}
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * Drops the host keys hortcmd stores for a profile's endpoint — the
 * explicit way out of a mismatch. Entries in the user's own
 * `known_hosts` files are reported, never rewritten.
 */
function ForgetHostKeyPanel() {
  const profile = useNet((s) => s.forgetting);
  const result = useNet((s) => s.forgot);
  const forget = useNet((s) => s.forgetHostKey);
  const showPanel = useNet((s) => s.showPanel);
  const t = useT();
  const [busy, setBusy] = useState(false);

  if (!profile) return null;

  const run = async () => {
    if (busy) return;
    setBusy(true);
    await forget();
    setBusy(false);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-auto">
      <div className="flex items-center gap-2 text-[13px] text-text">
        <ShieldOff size={15} className="text-amber-400" />
        {t("hostkey.forgetTitle")}
      </div>
      <p className="rounded border border-amber-500/40 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-400">
        {t("hostkey.forgetWarning")}
      </p>
      <p className="truncate font-mono text-[11px] text-dim">
        {profile.host}:{profile.port}
      </p>

      {result && (
        <div className="flex flex-col gap-1 rounded border border-edge bg-panel-inactive px-2.5 py-1.5 text-[11px] text-dim">
          <span>
            {t("hostkey.forgetDone").replace("{n}", String(result.removed))}
          </span>
          {result.remaining.map((file) => (
            <span key={file} className="break-all text-amber-400">
              {t("hostkey.forgetRemaining").replace("{file}", file)}
            </span>
          ))}
        </div>
      )}

      <div className="mt-1 flex justify-end gap-2">
        <button
          onClick={() => showPanel("list")}
          className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
        >
          {result ? t("hostkey.close") : t("net.cancel")}
        </button>
        {!result && (
          <button
            onClick={() => void run()}
            disabled={busy}
            className="rounded bg-red-600 px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
          >
            {t("hostkey.forgetConfirm")}
          </button>
        )}
      </div>
    </div>
  );
}

const inputCls =
  "w-full rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent";

function Field({
  label,
  className,
  children,
}: {
  label: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <label className={cn("flex flex-col gap-1", className)}>
      <span className="text-[11px] text-dim">{label}</span>
      {children}
    </label>
  );
}
