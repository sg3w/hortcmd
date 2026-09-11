// ============================================================
// Manage remote connections (SFTP, ticket 021).
//
// Two views inside one dialog: a list of saved connection profiles
// (connect / disconnect / edit / delete) and an add/edit form. Secrets
// are entered here but sent straight to the backend (OS keychain); the
// form never keeps them around after saving.
// ============================================================

import { useEffect, useState } from "react";
import {
  Pencil,
  Plug,
  PlugZap,
  Plus,
  Server,
  ShieldAlert,
  Trash2,
} from "lucide-react";
import type { Profile } from "@/ipc/bindings";
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
  const startEdit = useNet((s) => s.startEdit);
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
            <button
              onClick={() => startEdit(emptyProfile())}
              className="flex items-center gap-1.5 self-start rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
            >
              <Plus size={14} />
              {t("net.add")}
            </button>
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
