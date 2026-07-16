// ============================================================
// Dialog „Massenumbenennen": Namensschema mit Platzhaltern, Zähler,
// Suchen/Ersetzen (optional Regex) und Groß-/Kleinschreibung, mit
// Live-Vorschau (alt → neu) und Konflikterkennung. Das Umbenennen
// führt das Backend aus (`rename_batch`).
//
// Aufbau: oben die Konfiguration, darunter die Vorschau (volle Breite).
// Das Fenster ist unten rechts mit der Maus größenveränderbar.
// ============================================================

import { useEffect, useMemo, useState } from "react";
import {
  ArrowRight,
  CaseSensitive,
  Hash,
  PencilLine,
  Regex,
  Replace,
  Type,
} from "lucide-react";
import { panelOf, usePanes, type Row } from "@/store/panesStore";
import { useRenameDialog } from "@/store/renameStore";
import { useOps } from "@/store/opsStore";
import { useT } from "@/store/settingsStore";
import { renameBatch } from "@/ipc/client";
import type { TransKey } from "@/i18n/dictionaries";
import {
  buildPreview,
  buildRenamer,
  DEFAULT_RULES,
  type CaseMode,
  type RenamePreviewItem,
  type RenameRules,
} from "@/features/commander/renameRules";
import { Radio, Section } from "@/components/ui/dialogControls";
import { cn } from "@/lib/cn";
import { AppDialog } from "@/components/ui/AppDialog";

const CASE_OPTIONS: { value: CaseMode; key: TransKey }[] = [
  { value: "keep", key: "rename.case.keep" },
  { value: "lower", key: "rename.case.lower" },
  { value: "upper", key: "rename.case.upper" },
  { value: "title", key: "rename.case.title" },
];

const PREVIEW_LIMIT = 300;
const CONFLICT: RenamePreviewItem["status"][] = ["invalid", "duplicate", "exists"];

const DEFAULT_SIZE = { w: 860, h: 600 };
const MIN_W = 620;
const MIN_H = 420;

export function RenameDialog() {
  const side = useRenameDialog((s) => s.side);
  const close = useRenameDialog((s) => s.close);
  const t = useT();

  // Momentaufnahme der aktiven Liste beim Öffnen des Dialogs.
  const snapshot = useMemo(() => {
    if (!side) return null;
    const p = panelOf(usePanes.getState(), side);
    const entries = p.entries.filter((r) => !r.parent);
    const selectedCount = entries.filter((r) => p.selected.has(r.name)).length;
    return {
      entries,
      selected: p.selected,
      dir: p.path,
      folderNames: new Set(p.raw.map((e) => e.name)),
      selectedCount,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [side]);

  const [rules, setRules] = useState<RenameRules>(DEFAULT_RULES);
  const [onlySelected, setOnlySelected] = useState(false);

  useEffect(() => {
    if (!snapshot) return;
    setRules(DEFAULT_RULES);
    setOnlySelected(snapshot.selectedCount > 0);
  }, [snapshot]);

  const patch = (p: Partial<RenameRules>) => setRules((r) => ({ ...r, ...p }));

  const targets: Row[] = useMemo(() => {
    if (!snapshot) return [];
    return onlySelected
      ? snapshot.entries.filter((r) => snapshot.selected.has(r.name))
      : snapshot.entries;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot, onlySelected]);

  const { renamer, error } = useMemo(() => {
    const built = buildRenamer(rules);
    return { renamer: built.rename, error: built.error };
  }, [rules]);

  const preview = useMemo(() => {
    if (!snapshot || !renamer) return [];
    return buildPreview(targets, snapshot.folderNames, renamer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot, targets, renamer]);

  const changed = preview.filter((p) => p.status === "ok").length;
  const conflicts = preview.filter((p) => CONFLICT.includes(p.status)).length;

  const apply = async () => {
    if (!snapshot) return;
    const items = preview
      .filter((p) => p.status === "ok")
      .map((p) => [p.from, p.to] as [string, string]);
    if (!items.length) return;

    const res = await renameBatch(snapshot.dir, items);

    // Betroffene Fenster mit diesem Ordner aktualisieren.
    const s = usePanes.getState();
    (["left", "right"] as const).forEach((sd) => {
      if (panelOf(s, sd).path === snapshot.dir) void s.refresh(sd);
    });

    if (res.errors.length) {
      const message = res.errors.slice(0, 12).join("\n");
      useOps.getState().requestConfirm({
        title: t("op.errors"),
        message,
        onConfirm: () => {},
      });
    }
    close();
  };

  return (
    <AppDialog
      dialogKey="rename"
      open={!!side}
      onClose={close}
      titleBar={
        <>
          <Replace size={15} className="text-accent" />
          {t("rename.title")}
        </>
      }
      defaultSize={DEFAULT_SIZE}
      minSize={{ w: MIN_W, h: MIN_H }}
    >
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden p-4">
            {/* Konfiguration – kompakt, bricht bei Bedarf um. */}
            <div className="flex flex-wrap items-start gap-x-6 gap-y-4">
              <Section label={t("export.scope")}>
                <div className="flex flex-col gap-1.5">
                  <Radio
                    checked={!onlySelected}
                    onChange={() => setOnlySelected(false)}
                    label={t("export.scope.all").replace(
                      "{n}",
                      String(snapshot?.entries.length ?? 0),
                    )}
                  />
                  <Radio
                    checked={onlySelected}
                    onChange={() => setOnlySelected(true)}
                    disabled={!snapshot?.selectedCount}
                    label={t("export.scope.selected").replace(
                      "{n}",
                      String(snapshot?.selectedCount ?? 0),
                    )}
                  />
                </div>
              </Section>

              <div className="min-w-[240px] flex-1">
                <Section label={t("rename.schema")}>
                  <div className="flex items-center gap-2">
                    <PencilLine size={14} className="shrink-0 text-dim" />
                    <input
                      value={rules.nameMask}
                      onChange={(e) => patch({ nameMask: e.target.value })}
                      placeholder={t("rename.name")}
                      spellCheck={false}
                      className={inputCls}
                    />
                    <span className="text-dim">.</span>
                    <input
                      value={rules.extMask}
                      onChange={(e) => patch({ extMask: e.target.value })}
                      placeholder={t("rename.ext")}
                      spellCheck={false}
                      className={cn(inputCls, "w-24 shrink-0 flex-none")}
                    />
                  </div>
                  <span className="font-mono text-[11px] text-dim">
                    {t("rename.placeholders")}
                  </span>
                </Section>
              </div>

              <div className="min-w-[280px] flex-1">
                <Section label={t("rename.searchReplace")}>
                  <div className="flex items-center gap-2">
                    <Replace size={14} className="shrink-0 text-dim" />
                    <input
                      value={rules.search}
                      onChange={(e) => patch({ search: e.target.value })}
                      placeholder={t("rename.search")}
                      spellCheck={false}
                      className={inputCls}
                    />
                    <ArrowRight size={13} className="shrink-0 text-dim" />
                    <input
                      value={rules.replace}
                      onChange={(e) => patch({ replace: e.target.value })}
                      placeholder={t("rename.replace")}
                      spellCheck={false}
                      className={inputCls}
                    />
                  </div>
                  <div className="flex gap-2">
                    <IconToggle
                      active={rules.regex}
                      onClick={() => patch({ regex: !rules.regex })}
                      Icon={Regex}
                      label={t("rename.regex")}
                    />
                    <IconToggle
                      active={rules.caseSensitive}
                      onClick={() => patch({ caseSensitive: !rules.caseSensitive })}
                      Icon={CaseSensitive}
                      label={t("rename.caseSensitive")}
                    />
                  </div>
                  {error && (
                    <span className="text-[11px] text-red-400">
                      {t("rename.regexError").replace("{err}", error)}
                    </span>
                  )}
                </Section>
              </div>

              <Section label={t("rename.counter")}>
                <div className="flex items-center gap-3">
                  <Hash size={14} className="shrink-0 text-dim" />
                  <NumField
                    label={t("rename.counter.start")}
                    value={rules.counterStart}
                    onChange={(counterStart) => patch({ counterStart })}
                  />
                  <NumField
                    label={t("rename.counter.step")}
                    value={rules.counterStep}
                    onChange={(counterStep) => patch({ counterStep })}
                  />
                  <NumField
                    label={t("rename.counter.digits")}
                    value={rules.counterDigits}
                    min={1}
                    onChange={(counterDigits) => patch({ counterDigits })}
                  />
                </div>
              </Section>

              <Section label={t("rename.case")}>
                <div className="inline-flex items-center gap-2">
                  <Type size={14} className="shrink-0 text-dim" />
                  <div className="inline-flex overflow-hidden rounded border border-edge">
                    {CASE_OPTIONS.map((o) => (
                      <button
                        key={o.value}
                        onClick={() => patch({ caseMode: o.value })}
                        className={cn(
                          "px-2.5 py-1 text-[12px]",
                          rules.caseMode === o.value
                            ? "bg-accent text-white"
                            : "bg-panel text-dim hover:text-text",
                        )}
                      >
                        {t(o.key)}
                      </button>
                    ))}
                  </div>
                </div>
              </Section>
            </div>

            {/* Vorschau (volle Breite, füllt den restlichen Platz). */}
            <div className="flex min-h-0 flex-1 flex-col">
              <span className="mb-1.5 text-[13px] text-text">
                {t("rename.preview")}
              </span>
              <div className="min-h-0 flex-1 overflow-auto rounded border border-edge bg-panel-inactive">
                {preview.length === 0 ? (
                  <div className="p-3 text-[12px] text-dim">
                    {t("rename.preview.empty")}
                  </div>
                ) : (
                  <table className="w-full table-fixed border-collapse font-mono text-[11px]">
                    <tbody>
                      {preview.slice(0, PREVIEW_LIMIT).map((p, i) => (
                        <tr key={i} className="border-b border-edge/50">
                          <td className="max-w-0 truncate px-2 py-0.5 text-dim">
                            {p.from}
                          </td>
                          <td className="w-6 px-1 text-center text-dim">→</td>
                          <td
                            className={cn(
                              "max-w-0 truncate px-2 py-0.5",
                              p.status === "unchanged" && "text-dim",
                              p.status === "ok" && "text-text",
                              CONFLICT.includes(p.status) &&
                                "text-red-400 line-through decoration-red-400/50",
                            )}
                            title={
                              CONFLICT.includes(p.status)
                                ? t(`rename.conflict.${p.status}` as TransKey)
                                : undefined
                            }
                          >
                            {p.to || "∅"}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
                {preview.length > PREVIEW_LIMIT && (
                  <div className="px-2 py-1 text-[11px] text-dim">
                    {t("rename.preview.more").replace(
                      "{n}",
                      String(preview.length - PREVIEW_LIMIT),
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Fußzeile */}
          <div className="flex items-center gap-3 border-t border-edge px-4 py-2.5">
            <span className="text-[12px] text-dim">
              {t("rename.count")
                .replace("{changed}", String(changed))
                .replace("{total}", String(preview.length))}
            </span>
            {conflicts > 0 && (
              <span className="text-[12px] text-red-400">
                {t("rename.conflicts").replace("{n}", String(conflicts))}
              </span>
            )}
            <div className="ml-auto flex gap-2">
              <button
                onClick={close}
                className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
              >
                {t("op.cancel")}
              </button>
              <button
                onClick={apply}
                disabled={!!error || changed === 0}
                className="flex items-center gap-1.5 rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
              >
                <Replace size={14} />
                {t("rename.apply")}
              </button>
            </div>
          </div>

    </AppDialog>
  );
}

const inputCls =
  "min-w-0 flex-1 rounded border border-edge bg-panel px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent";

function IconToggle({
  active,
  onClick,
  Icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  Icon: typeof Regex;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      title={label}
      aria-pressed={active}
      className={cn(
        "flex items-center gap-1.5 rounded border px-2 py-1 text-[12px]",
        active
          ? "border-accent bg-accent-dim text-accent"
          : "border-edge text-dim hover:text-text",
      )}
    >
      <Icon size={14} />
      {label}
    </button>
  );
}

function NumField({
  label,
  value,
  onChange,
  min,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
  min?: number;
}) {
  return (
    <label className="flex items-center gap-1.5 text-[12px] text-dim">
      {label}
      <input
        type="number"
        value={value}
        min={min}
        onChange={(e) => {
          const n = parseInt(e.target.value, 10);
          onChange(Number.isNaN(n) ? (min ?? 0) : min != null ? Math.max(min, n) : n);
        }}
        className="w-14 rounded border border-edge bg-panel px-1.5 py-1 text-[12px] text-text outline-none focus:border-accent"
      />
    </label>
  );
}
