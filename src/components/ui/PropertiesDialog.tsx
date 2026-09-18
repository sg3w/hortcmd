// ============================================================
// "Properties / permissions" dialog (Alt+Enter or context menu).
// Shows and edits access permissions (chmod), owner/group (chown),
// lists extended attributes and ACL and computes checksums.
// Folder size and volume usage are calculated in the background
// (TICKET-010) and cancelled when the dialog closes.
// All logic runs in the backend; here only display/input.
// ============================================================

import { useDialogFocusReturn } from "./useDialogFocusReturn";
import { Fragment, useEffect, useMemo, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Copy, HardDrive, Info, Loader2, ShieldCheck, X } from "lucide-react";
import { usePropsDialog } from "@/store/propsStore";
import { useT } from "@/store/settingsStore";
import {
  cancelDirSize,
  dirSize,
  fileChecksums,
  fileProps,
  getTags,
  openSystemProperties,
  setOwner,
  setPermissions,
  setTags,
  volumeInfo,
} from "@/ipc/client";
import { reloadBoth } from "@/features/commander/fileOps";
import { formatDate, formatMode, formatPercent, formatSize } from "@/lib/format";
import { writeClipboard } from "@/lib/clipboard";
import { useCapability } from "@/store/capsStore";
import { explain } from "@/features/commander/guard";
import type {
  Checksums,
  DirSizeStats,
  FileProps,
  Tag,
  VolumeInfo,
} from "@/ipc/bindings";
import type { TransKey } from "@/i18n/dictionaries";
import { Section } from "@/components/ui/dialogControls";
import {
  fileColorLabel,
  fileColorRef,
  resolveFileColor,
} from "@/lib/fileColors";
import { cn } from "@/lib/cn";
import { consumeDialogKey, markDialogKey } from "@/lib/keys";
import { useFileColorRules } from "@/store/fileColorRules";

// Permission bits of the three classes (owner/group/other) and the special bits.
const CLASSES = [
  { key: "props.class.owner" as const, shift: 6 },
  { key: "props.class.group" as const, shift: 3 },
  { key: "props.class.other" as const, shift: 0 },
];
const BITS = [
  { key: "props.read" as const, bit: 4 },
  { key: "props.write" as const, bit: 2 },
  { key: "props.exec" as const, bit: 1 },
];
const SPECIAL = [
  { label: "setuid", bit: 0o4000 },
  { label: "setgid", bit: 0o2000 },
  { label: "sticky", bit: 0o1000 },
];

// Finder tag colors (index 1–7); 0 = no color (gray ring).
const TAG_COLORS: Record<number, string> = {
  0: "transparent",
  1: "#9aa0a6", // gray
  2: "#5fd25b", // green
  3: "#c470de", // purple
  4: "#4a9bf5", // blue
  5: "#f7cc46", // yellow
  6: "#f0524a", // red
  7: "#f5a43c", // orange
};

type T = (key: TransKey) => string;

// Each calculation gets its own token, so a cancel can never hit the
// task of a folder opened afterwards.
let nextSizeToken = 0;

/**
 * Recursive size of a folder. Runs in the backend, streams intermediate
 * totals, and is cancelled on unmount or when switching entries.
 * `path` = null (file, or properties not loaded yet) starts nothing.
 */
function useDirSize(path: string | null): {
  stats: DirSizeStats | null;
  running: boolean;
} {
  const [stats, setStats] = useState<DirSizeStats | null>(null);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    setStats(null);
    if (!path) {
      setRunning(false);
      return;
    }
    setRunning(true);
    const token = `dirsize:${++nextSizeToken}`;
    let alive = true;
    void dirSize(path, token, (s) => alive && setStats(s))
      .then((res) => {
        // A cancelled run only carries a partial total – discard it.
        if (!alive || res.cancelled) return;
        setStats(res.stats);
        setRunning(false);
      })
      .catch(() => alive && setRunning(false));
    return () => {
      alive = false;
      void cancelDirSize(token);
    };
  }, [path]);

  return { stats, running };
}

/** Volume of a path: `undefined` = still loading, `null` = none found. */
function useVolume(path: string | null): VolumeInfo | null | undefined {
  const [vol, setVol] = useState<VolumeInfo | null | undefined>(undefined);

  useEffect(() => {
    setVol(undefined);
    if (!path) return;
    let alive = true;
    void volumeInfo(path)
      .then((v) => alive && setVol(v))
      .catch(() => alive && setVol(null));
    return () => {
      alive = false;
    };
  }, [path]);

  return vol;
}

export function PropertiesDialog() {
  const returnFocus = useDialogFocusReturn();
  const path = usePropsDialog((s) => s.path);
  const gitStatus = usePropsDialog((s) => s.gitStatus);
  const close = usePropsDialog((s) => s.close);
  const t = useT();
  const colorRules = useFileColorRules();

  const [data, setData] = useState<FileProps | null>(null);
  const [perm, setPerm] = useState(0); // permission bits (0..0o7777)
  const [owner, setOwnerText] = useState("");
  const [group, setGroupText] = useState("");
  const [checks, setChecks] = useState<Checksums | null>(null);
  const [computing, setComputing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [tags, setTagList] = useState<Tag[]>([]);
  const [sysBusy, setSysBusy] = useState(false);

  // What this location supports (TICKET-031). Every section below asks
  // the contract instead of testing the platform or the scheme itself.
  const capPath = path ?? "";
  const canTags = useCapability(capPath, "finderTags");
  const canPerms = useCapability(capPath, "permissions");
  const canSystemOpen = useCapability(capPath, "systemOpen");
  const canSize = useCapability(capPath, "recursiveSize");
  const canVolume = useCapability(capPath, "volumeInfo");
  const canChecksums = useCapability(capPath, "checksums");

  // Both derive from the loaded properties, not from `path`: only then is
  // it certain whether the entry is a folder, and both restart cleanly
  // when switching entries.
  const size = useDirSize(
    data?.is_dir && canSize ? data.path : null,
  );
  const volume = useVolume(canVolume ? (data?.path ?? null) : null);

  // On open (or path change), load the properties. Closing the dialog
  // (path = null) must drop `data` too: the folder-size task and the
  // volume lookup key off it, and only a cleared entry cancels them.
  useEffect(() => {
    setData(null);
    setChecks(null);
    setError(null);
    setTagList([]);
    setSysBusy(false);
    if (!path) return;
    let alive = true;
    void fileProps(path)
      .then((p) => {
        if (!alive) return;
        setData(p);
        setPerm((p.mode ?? 0) & 0o7777);
        setOwnerText(p.owner ?? (p.uid != null ? String(p.uid) : ""));
        setGroupText(p.group ?? (p.gid != null ? String(p.gid) : ""));
      })
      .catch((e) => alive && setError(String(e)));
    // Finder tags are a capability of the location, not merely of the
    // platform (TICKET-031) — a remote folder on a Mac has none.
    if (canTags) {
      void getTags(path).then((tg) => alive && setTagList(tg));
    }
    return () => {
      alive = false;
    };
  }, [path]);

  // Save tags and update the local state.
  const persistTags = async (next: Tag[]) => {
    if (!path) return;
    setTagList(next);
    try {
      await setTags(path, next);
    } catch (e) {
      setError(String(e));
    }
  };

  const octal = useMemo(() => perm.toString(8).padStart(4, "0"), [perm]);

  const toggle = (bit: number) => setPerm((m) => m ^ bit);

  const applyPerms = async () => {
    if (!path) return;
    try {
      await setPermissions(path, perm);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const applyOwner = async () => {
    if (!path || !data) return;
    try {
      // Only send changed fields.
      const o = owner !== (data.owner ?? String(data.uid ?? "")) ? owner : null;
      const g = group !== (data.group ?? String(data.gid ?? "")) ? group : null;
      await setOwner(path, o, g);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  // After a change: reload the properties and refresh both lists.
  const refresh = async () => {
    if (!path) return;
    setError(null);
    const p = await fileProps(path);
    setData(p);
    setPerm((p.mode ?? 0) & 0o7777);
    setOwnerText(p.owner ?? (p.uid != null ? String(p.uid) : ""));
    setGroupText(p.group ?? (p.gid != null ? String(p.gid) : ""));
    void reloadBoth();
  };

  const compute = async () => {
    if (!path) return;
    setComputing(true);
    try {
      setChecks(await fileChecksums(path));
    } catch (e) {
      setError(String(e));
    } finally {
      setComputing(false);
    }
  };

  const copy = async (value: string, tag: string) => {
    try {
      await writeClipboard(value);
      setCopied(tag);
      setTimeout(() => setCopied((c) => (c === tag ? null : c)), 1200);
    } catch (error) {
      setCopied(null);
      setError(t("sel.copyError").replace("{err}", String(error)));
    }
  };

  // The rule that colors this entry in the file list. Same registry and the
  // same context the list evaluates, so the dialog cannot disagree with it.
  const colorRule = useMemo(
    () =>
      data ? resolveFileColor(colorRules, { ...data, gitStatus }) : undefined,
    [colorRules, data, gitStatus],
  );

  // Hand-off to the OS information window. Only a local file-system path
  // has one, so for a remote VFS address the button stays disabled and
  // says why. Archive members never get here at all — both entry points
  // (Alt+Enter in `useKeyboard`, the context menu item) already refuse to
  // open this dialog while a pane is inside an archive.
  const sysReason = !path
    ? null
    : canSystemOpen
      ? null
      : (explain(path, "systemOpen") ?? t("props.system.remote"));

  const openSystem = async () => {
    if (!path || sysReason) return;
    setSysBusy(true);
    setError(null);
    try {
      await openSystemProperties(path);
    } catch (e) {
      setError(String(e));
    } finally {
      // The native window outlives this call; the button is free again
      // either way, also after a failure.
      setSysBusy(false);
    }
  };

  const typeLabel = (p: FileProps) =>
    p.is_symlink
      ? t("props.type.symlink")
      : p.is_dir
        ? t("props.type.dir")
        : t("props.type.file");

  return (
    <Dialog.Root open={!!path} onOpenChange={(o) => !o && close()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/50" />
        <Dialog.Content
          onCloseAutoFocus={returnFocus}
          onKeyDownCapture={markDialogKey}
          aria-describedby={undefined}
          className="fixed left-1/2 top-1/2 z-50 flex max-h-[92vh] w-[560px] max-w-[96vw] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-edge bg-panel shadow-2xl"
        >
          <Dialog.Title className="flex items-center gap-2 border-b border-edge bg-header px-4 py-2 text-text">
            <ShieldCheck size={16} className="text-dim" aria-hidden />
            <span className="truncate text-[13px] font-medium">
              {t("props.title")}
              {data ? ` — ${data.name}` : ""}
            </span>
          </Dialog.Title>

          <div className="flex min-h-0 flex-col gap-4 overflow-auto p-4">
            <p className="text-sm text-dim">{t("undo.conditional")}</p>
            {!data && !error && (
              <div className="flex items-center gap-2 text-[13px] text-dim">
                <Loader2 size={14} className="animate-spin" aria-hidden />
                …
              </div>
            )}

            {error && (
              <div className="rounded border border-red-500/50 bg-red-500/10 px-3 py-2 text-[12px] text-red-400">
                {error}
              </div>
            )}

            {data && (
              <>
                {/* Header data */}
                <div className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[12px]">
                  <span className="text-dim">{t("props.type.file")}</span>
                  <span className="text-text">{typeLabel(data)}</span>
                  <span className="text-dim">{t("props.size")}</span>
                  {data.is_dir ? (
                    <FolderSize stats={size.stats} running={size.running} t={t} />
                  ) : (
                    <span className="text-text">
                      {formatSize(data.size)} ({formatSize(data.size, "bytes")})
                    </span>
                  )}
                  <span className="text-dim">{t("props.modified")}</span>
                  <span className="text-text">{formatDate(data.modified)}</span>
                  {data.mode != null && (
                    <>
                      <span className="text-dim">{t("props.mode")}</span>
                      <span className="font-mono text-text">
                        {formatMode(data.mode, data.is_dir, data.is_symlink)} ·{" "}
                        {octal}
                      </span>
                    </>
                  )}
                </div>

                {/* Storage of the volume this entry lies on */}
                <Section label={t("vol.title")}>
                  <Volume vol={volume} t={t} />
                </Section>

                {/* Why the entry has its color in the file list */}
                <Section label={t("props.color")}>
                  {colorRule ? (
                    <div className="flex flex-wrap items-center gap-2">
                      <span
                        className="h-3.5 w-3.5 shrink-0 rounded-sm border border-edge"
                        style={{ background: fileColorRef(colorRule.id) }}
                        aria-hidden
                      />
                      <span
                        className={cn("font-mono text-[12px]", colorRule.extraClass)}
                        style={{ color: fileColorRef(colorRule.id) }}
                      >
                        {data.name}
                      </span>
                      <span className="text-[12px] text-dim">
                        — {fileColorLabel(colorRule, t)}
                      </span>
                    </div>
                  ) : (
                    <span className="text-[12px] text-dim">
                      {t("props.color.none")}
                    </span>
                  )}
                </Section>

                {/* Finder tags — where the location offers them */}
                {canTags && (
                  <Section label={t("props.tags")}>
                    <TagEditor
                      tags={tags}
                      onChange={persistTags}
                      addLabel={t("props.tags.add")}
                    />
                  </Section>
                )}

                {!canPerms ? (
                  <p className="text-[12px] text-dim">{t("props.unsupported")}</p>
                ) : (
                  <>
                    {/* Access permissions */}
                    <Section label={t("props.perms")}>
                      <div className="overflow-hidden rounded border border-edge">
                        <table className="w-full text-[12px]">
                          <thead>
                            <tr className="bg-panel-inactive text-dim">
                              <th className="px-2 py-1 text-left font-normal" />
                              {BITS.map((b) => (
                                <th
                                  key={b.key}
                                  className="px-2 py-1 text-center font-normal"
                                >
                                  {t(b.key)}
                                </th>
                              ))}
                            </tr>
                          </thead>
                          <tbody>
                            {CLASSES.map((c) => (
                              <tr key={c.key} className="border-t border-edge">
                                <td className="px-2 py-1 text-dim">
                                  {t(c.key)}
                                </td>
                                {BITS.map((b) => {
                                  const mask = b.bit << c.shift;
                                  return (
                                    <td key={b.key} className="text-center">
                                      <input
                                        type="checkbox"
                                        checked={(perm & mask) !== 0}
                                        onChange={() => toggle(mask)}
                                        className="accent-[color:var(--accent)]"
                                        aria-label={`${t(c.key)} ${t(b.key)}`}
                                      />
                                    </td>
                                  );
                                })}
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>

                      <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
                        {SPECIAL.map((s) => (
                          <label
                            key={s.label}
                            className="flex cursor-pointer items-center gap-1.5 text-[12px] text-text"
                          >
                            <input
                              type="checkbox"
                              checked={(perm & s.bit) !== 0}
                              onChange={() => toggle(s.bit)}
                              className="accent-[color:var(--accent)]"
                            />
                            {s.label}
                          </label>
                        ))}
                        <label className="ml-auto flex items-center gap-1.5 text-[12px] text-dim">
                          {t("props.octal")}
                          <input
                            value={octal}
                            onChange={(e) => {
                              const v = parseInt(e.target.value || "0", 8);
                              if (!Number.isNaN(v)) setPerm(v & 0o7777);
                            }}
                            className="w-16 rounded border border-edge bg-panel px-2 py-0.5 text-center font-mono text-[12px] text-text outline-none focus:border-accent"
                          />
                        </label>
                        <button
                          onClick={applyPerms}
                          className="rounded bg-accent px-3 py-1 text-[12px] text-white hover:brightness-110"
                        >
                          {t("props.apply")}
                        </button>
                      </div>
                    </Section>

                    {/* Owner / group */}
                    <Section label={t("props.owner")}>
                      <div className="flex flex-wrap items-end gap-2">
                        <label className="flex flex-col gap-1 text-[11px] text-dim">
                          {t("props.owner")}
                          <input
                            value={owner}
                            onChange={(e) => setOwnerText(e.target.value)}
                            className="w-40 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
                          />
                        </label>
                        <label className="flex flex-col gap-1 text-[11px] text-dim">
                          {t("props.group")}
                          <input
                            value={group}
                            onChange={(e) => setGroupText(e.target.value)}
                            className="w-40 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
                          />
                        </label>
                        <button
                          onClick={applyOwner}
                          className="rounded border border-edge bg-panel px-3 py-1 text-[12px] text-text hover:border-accent"
                        >
                          {t("props.change")}
                        </button>
                      </div>
                    </Section>

                    {/* Extended Attributes */}
                    <Section label={t("props.xattr")}>
                      {data.xattrs.length === 0 ? (
                        <span className="text-[12px] text-dim">
                          {t("props.empty")}
                        </span>
                      ) : (
                        <ul className="flex flex-col gap-1 text-[12px]">
                          {data.xattrs.map((x) => (
                            <li
                              key={x.name}
                              className="flex items-baseline gap-2"
                            >
                              <span className="font-mono text-text">
                                {x.name}
                              </span>
                              <span className="text-dim">({x.size} B)</span>
                              {x.value && (
                                <span className="truncate font-mono text-dim">
                                  {x.value}
                                </span>
                              )}
                            </li>
                          ))}
                        </ul>
                      )}
                    </Section>

                    {/* ACL */}
                    <Section label={t("props.acl")}>
                      {data.acl.length === 0 ? (
                        <span className="text-[12px] text-dim">
                          {t("props.empty")}
                        </span>
                      ) : (
                        <pre className="overflow-auto rounded border border-edge bg-panel-inactive p-2 font-mono text-[11px] text-text">
                          {data.acl.join("\n")}
                        </pre>
                      )}
                    </Section>
                  </>
                )}

                {/* Checksums (files only) */}
                {!data.is_dir && (
                  <Section label={t("props.checksums")}>
                    {checks ? (
                      <div className="grid grid-cols-[auto_1fr_auto] items-center gap-x-3 gap-y-1 text-[12px]">
                        {(
                          [
                            ["MD5", checks.md5],
                            ["SHA-1", checks.sha1],
                            ["SHA-256", checks.sha256],
                          ] as const
                        ).map(([label, value]) => (
                          <RowSum
                            key={label}
                            label={label}
                            value={value}
                            copied={copied === label}
                            onCopy={() => copy(value, label)}
                            copyTitle={t("props.copy")}
                          />
                        ))}
                      </div>
                    ) : (
                      <button
                        onClick={compute}
                        disabled={computing || !canChecksums}
                        title={
                          canChecksums
                            ? undefined
                            : (explain(capPath, "checksums") ?? undefined)
                        }
                        className="flex items-center gap-1.5 self-start rounded border border-edge bg-panel px-3 py-1 text-[12px] text-text hover:border-accent disabled:opacity-50"
                      >
                        {computing && (
                          <Loader2 size={13} className="animate-spin" aria-hidden />
                        )}
                        {computing ? t("props.computing") : t("props.compute")}
                      </button>
                    )}
                  </Section>
                )}
              </>
            )}
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-edge bg-header px-4 py-2">
            <button
              onClick={() => void openSystem()}
              disabled={!data || !!sysReason || sysBusy}
              title={sysReason ?? t("props.system.hint")}
              className="flex items-center gap-1.5 rounded border border-edge bg-panel px-3 py-1 text-[12px] text-text hover:border-accent disabled:cursor-not-allowed disabled:opacity-50"
            >
              {sysBusy ? (
                <Loader2 size={13} className="animate-spin" aria-hidden />
              ) : (
                <Info size={13} aria-hidden />
              )}
              {t("props.system")}
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/**
 * Size of a folder: while the walk runs, the totals grow visibly next to
 * a spinner; unreadable entries are reported instead of aborting.
 */
function FolderSize({
  stats,
  running,
  t,
}: {
  stats: DirSizeStats | null;
  running: boolean;
  t: T;
}) {
  if (!stats) {
    return (
      <span className="flex items-center gap-1.5 text-dim">
        {running ? (
          <>
            <Loader2 size={12} className="animate-spin" aria-hidden />
            {t("props.calculating")}
          </>
        ) : (
          "—"
        )}
      </span>
    );
  }
  return (
    <span className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
      {running && (
        <Loader2 size={12} className="animate-spin text-dim" aria-hidden />
      )}
      <span className="text-text">
        {formatSize(stats.bytes)} ({formatSize(stats.bytes, "bytes")})
      </span>
      <span className="text-dim">
        {stats.files} {t("props.files")} · {stats.dirs} {t("props.folders")}
        {stats.errors > 0 && ` · ${stats.errors} ${t("props.skipped")}`}
      </span>
    </span>
  );
}

/** Capacity and usage of a volume, with "40 G free of 500 G" up front. */
function Volume({ vol, t }: { vol: VolumeInfo | null | undefined; t: T }) {
  if (vol === undefined) {
    return (
      <span className="flex items-center gap-1.5 text-[12px] text-dim">
        <Loader2 size={12} className="animate-spin" aria-hidden />
        {t("props.calculating")}
      </span>
    );
  }
  if (!vol) {
    return <span className="text-[12px] text-dim">{t("vol.none")}</span>;
  }

  const rows: [string, string][] = [
    [t("vol.name"), vol.name],
    [t("vol.fs"), vol.fs || "—"],
    [t("vol.total"), formatSize(vol.total)],
    [t("vol.used"), `${formatSize(vol.used)} (${formatPercent(vol.used_pct)})`],
    [t("vol.free"), `${formatSize(vol.free)} (${formatPercent(vol.free_pct)})`],
  ];

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-baseline gap-x-2">
        <HardDrive size={14} className="self-center text-accent" aria-hidden />
        <span className="text-[13px] font-medium text-text">
          {formatSize(vol.free)} {t("vol.freeOf")} {formatSize(vol.total)}
        </span>
        <span className="text-[12px] text-dim">
          ({formatPercent(vol.free_pct)})
        </span>
      </div>

      <div className="h-1.5 overflow-hidden rounded-full bg-panel-inactive">
        <div
          className={cn(
            "h-full rounded-full",
            vol.free_pct < 10 ? "bg-red-500" : "bg-accent",
          )}
          style={{ width: `${Math.min(100, vol.used_pct)}%` }}
        />
      </div>

      <div className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[12px]">
        <span className="text-dim">{t("vol.mount")}</span>
        <span className="truncate font-mono text-text" title={vol.mount}>
          {vol.mount}
        </span>
        {rows.map(([label, value]) => (
          <Fragment key={label}>
            <span className="text-dim">{label}</span>
            <span className="truncate text-text" title={value}>
              {value}
            </span>
          </Fragment>
        ))}
      </div>
    </div>
  );
}

function RowSum({
  label,
  value,
  copied,
  onCopy,
  copyTitle,
}: {
  label: string;
  value: string;
  copied: boolean;
  onCopy: () => void;
  copyTitle: string;
}) {
  return (
    <>
      <span className="text-dim">{label}</span>
      <span className="truncate font-mono text-text" title={value}>
        {value}
      </span>
      <button
        onClick={onCopy}
        title={copyTitle}
        className="text-dim hover:text-text"
      >
        <Copy size={13} className={copied ? "text-accent" : ""} aria-hidden />
      </button>
    </>
  );
}

/** A colored dot for a Finder tag (0 = empty ring). */
function ColorDot({ color, size = 12 }: { color: number; size?: number }) {
  const c = TAG_COLORS[color] ?? "transparent";
  return (
    <span
      className="inline-block shrink-0 rounded-full border"
      style={{
        width: size,
        height: size,
        background: color === 0 ? "transparent" : c,
        borderColor: color === 0 ? "var(--text-dim)" : c,
      }}
      aria-hidden
    />
  );
}

/** Display + editing of an entry's Finder tags. */
function TagEditor({
  tags,
  onChange,
  addLabel,
}: {
  tags: Tag[];
  onChange: (next: Tag[]) => void;
  addLabel: string;
}) {
  const translate = useT();
  const [name, setName] = useState("");
  const [color, setColor] = useState(0);

  const add = () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    // Do not duplicate an existing name; update only the color if needed.
    const next = tags.some((t) => t.name === trimmed)
      ? tags.map((t) => (t.name === trimmed ? { name: trimmed, color } : t))
      : [...tags, { name: trimmed, color }];
    onChange(next);
    setName("");
    setColor(0);
  };

  const remove = (n: string) =>
    onChange(tags.filter((t) => t.name !== n));

  return (
    <div className="flex flex-col gap-2">
      {tags.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {tags.map((t) => (
            <span
              key={t.name}
              className="flex items-center gap-1.5 rounded-full border border-edge bg-panel-inactive py-0.5 pl-2 pr-1 text-[12px] text-text"
            >
              <ColorDot color={t.color} />
              {t.name}
              <button
                onClick={() => remove(t.name)}
                className="rounded-full p-0.5 text-dim hover:bg-red-500/20 hover:text-red-400"
                aria-label={translate("props.tags.remove").replace("{name}", t.name)}
              >
                <X size={12} aria-hidden />
              </button>
            </span>
          ))}
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter") return;
            consumeDialogKey(e);
            add();
          }}
          placeholder={addLabel}
          aria-label={addLabel}
          className="w-40 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
        />
        <div className="flex items-center gap-1">
          {[0, 1, 2, 3, 4, 5, 6, 7].map((c) => (
            <button
              key={c}
              onClick={() => setColor(c)}
              aria-label={translate("props.tags.color").replace("{n}", String(c))}
              aria-pressed={color === c}
              className={cn(
                "rounded-full p-0.5",
                color === c && "ring-2 ring-accent",
              )}
            >
              <ColorDot color={c} size={14} />
            </button>
          ))}
        </div>
        <button
          onClick={add}
          aria-label={addLabel}
          disabled={!name.trim()}
          className="rounded border border-edge bg-panel px-3 py-1 text-[12px] text-text hover:border-accent disabled:opacity-40"
        >
          +
        </button>
      </div>
    </div>
  );
}
