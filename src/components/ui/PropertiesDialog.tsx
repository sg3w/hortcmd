// ============================================================
// "Properties / permissions" dialog (Alt+Enter or context menu).
// Shows and edits access permissions (chmod), owner/group (chown),
// lists extended attributes and ACL and computes checksums.
// All logic runs in the backend; here only display/input.
// ============================================================

import { useEffect, useMemo, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Copy, Loader2, ShieldCheck, X } from "lucide-react";
import { usePropsDialog } from "@/store/propsStore";
import { useT } from "@/store/settingsStore";
import {
  fileChecksums,
  fileProps,
  getTags,
  setOwner,
  setPermissions,
  setTags,
} from "@/ipc/client";
import { reloadBoth } from "@/features/commander/fileOps";
import { formatDate, formatMode, formatSize } from "@/lib/format";
import { writeClipboard } from "@/lib/clipboard";
import { isMacOS } from "@/lib/platform";
import type { Checksums, FileProps, Tag } from "@/ipc/bindings";
import { Section } from "@/components/ui/dialogControls";
import {
  fileColorLabel,
  fileColorRef,
  resolveFileColor,
  useFileColorRules,
} from "@/lib/fileColors";
import { cn } from "@/lib/cn";

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

export function PropertiesDialog() {
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

  // On open (or path change), load the properties.
  useEffect(() => {
    if (!path) return;
    setData(null);
    setChecks(null);
    setError(null);
    setTagList([]);
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
    if (isMacOS) {
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
    await writeClipboard(value);
    setCopied(tag);
    setTimeout(() => setCopied((c) => (c === tag ? null : c)), 1200);
  };

  // The rule that colors this entry in the file list. Same registry and the
  // same context the list evaluates, so the dialog cannot disagree with it.
  const colorRule = useMemo(
    () =>
      data ? resolveFileColor(colorRules, { ...data, gitStatus }) : undefined,
    [colorRules, data, gitStatus],
  );

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
                  <span className="text-dim">Größe</span>
                  <span className="text-text">
                    {formatSize(data.size)} ({data.size} B)
                  </span>
                  <span className="text-dim">Geändert</span>
                  <span className="text-text">{formatDate(data.modified)}</span>
                  {data.mode != null && (
                    <>
                      <span className="text-dim">Modus</span>
                      <span className="font-mono text-text">
                        {formatMode(data.mode, data.is_dir, data.is_symlink)} ·{" "}
                        {octal}
                      </span>
                    </>
                  )}
                </div>

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

                {/* Finder tags (macOS only) */}
                {isMacOS && (
                  <Section label={t("props.tags")}>
                    <TagEditor
                      tags={tags}
                      onChange={persistTags}
                      addLabel={t("props.tags.add")}
                    />
                  </Section>
                )}

                {!data.unix ? (
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
                        disabled={computing}
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
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
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
                aria-label={`${t.name} entfernen`}
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
          onKeyDown={(e) => e.key === "Enter" && add()}
          placeholder={addLabel}
          className="w-40 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
        />
        <div className="flex items-center gap-1">
          {[0, 1, 2, 3, 4, 5, 6, 7].map((c) => (
            <button
              key={c}
              onClick={() => setColor(c)}
              title={`Farbe ${c}`}
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
          disabled={!name.trim()}
          className="rounded border border-edge bg-panel px-3 py-1 text-[12px] text-text hover:border-accent disabled:opacity-40"
        >
          +
        </button>
      </div>
    </div>
  );
}
