// ============================================================
// Dialog „Dateiliste exportieren": konfiguriert Umfang, Format,
// Felder und Optionen, zeigt eine Live-Vorschau und schreibt das
// Ergebnis via Backend (`write_text_file`) bzw. in die Zwischenablage.
// ============================================================

import { useEffect, useMemo, useState } from "react";
import {
  ClipboardCopy,
  FileWarning,
  FolderSearch,
  ListTree,
  Save,
} from "lucide-react";
import { panelOf, usePanes } from "@/store/panesStore";
import { useExportDialog } from "@/store/exportStore";
import { openFileBrowser } from "@/store/fileBrowserStore";
import { useOps } from "@/store/opsStore";
import { useSettings, useT } from "@/store/settingsStore";
import { writeTextFile } from "@/ipc/client";
import { writeClipboard } from "@/lib/clipboard";
import { baseName, joinPath, parentPath } from "@/lib/path";
import { Check, Radio, Section } from "@/components/ui/dialogControls";
import type { TransKey } from "@/i18n/dictionaries";
import {
  buildExport,
  EXPORT_FIELDS,
  FORMAT_EXT,
  pickRows,
  type ExportField,
  type ExportFormat,
} from "@/features/commander/exportList";
import { cn } from "@/lib/cn";
import { AppDialog } from "@/components/ui/AppDialog";

const FORMATS: { value: ExportFormat; key: TransKey }[] = [
  { value: "list", key: "export.format.list" },
  { value: "csv", key: "export.format.csv" },
  { value: "tsv", key: "export.format.tsv" },
  { value: "json", key: "export.format.json" },
  { value: "xml", key: "export.format.xml" },
];

const FIELD_LABEL: Record<ExportField, TransKey> = {
  path: "export.field.path",
  name: "export.field.name",
  ext: "export.field.ext",
  size: "export.field.size",
  modified: "export.field.modified",
  type: "export.field.type",
  perms: "export.field.perms",
};

const PREVIEW_ROWS = 100;

const DEFAULT_SIZE = { w: 820, h: 600 };
const MIN_W = 600;
const MIN_H = 420;

/** Ersetzt die Endung eines Dateinamens (behält den Basisnamen). */
function withExt(name: string, ext: string): string {
  const base = name.replace(/\.[^.\\/]*$/, "");
  return `${base}.${ext}`;
}

export function ExportDialog() {
  const side = useExportDialog((s) => s.side);
  const close = useExportDialog((s) => s.close);
  const t = useT();
  const sizeFormat = useSettings((s) => s.sizeFormat);
  const dateFormat = useSettings((s) => s.dateFormat);

  // Momentaufnahme der aktiven Liste beim Öffnen des Dialogs.
  const snapshot = useMemo(() => {
    if (!side) return null;
    const p = panelOf(usePanes.getState(), side);
    const entries = p.entries.filter((r) => !r.parent);
    const selectedCount = entries.filter((r) => p.selected.has(r.name)).length;
    const destDir = p.archive ? parentPath(p.archive) : p.path;
    return {
      rows: p.entries,
      selected: p.selected,
      dir: p.path,
      total: entries.length,
      selectedCount,
      destDir,
    };
    // Bewusst nur an `side` gebunden: Snapshot bei jedem Öffnen neu.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [side]);

  const [format, setFormat] = useState<ExportFormat>("list");
  const [fields, setFields] = useState<Set<ExportField>>(new Set(["path"]));
  const [onlySelected, setOnlySelected] = useState(false);
  const [includeFolders, setIncludeFolders] = useState(true);
  const [header, setHeader] = useState(true);
  const [formatValues, setFormatValues] = useState(true);
  const [fileName, setFileName] = useState("Dateiliste.txt");
  const [destDir, setDestDir] = useState("");
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(
    null,
  );

  // Beim Öffnen die Standardwerte aus dem Snapshot herstellen.
  useEffect(() => {
    if (!snapshot) return;
    setFormat("list");
    setFields(new Set(["path"]));
    setOnlySelected(snapshot.selectedCount > 0);
    setIncludeFolders(true);
    setHeader(true);
    setFormatValues(true);
    setFileName("Dateiliste.txt");
    setDestDir(snapshot.destDir);
    setStatus(null);
  }, [snapshot]);

  const orderedFields = useMemo(
    () => EXPORT_FIELDS.filter((f) => fields.has(f)),
    [fields],
  );

  const rows = useMemo(() => {
    if (!snapshot) return [];
    return pickRows(snapshot.rows, snapshot.selected, {
      onlySelected,
      includeFolders,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot, onlySelected, includeFolders]);

  const options = {
    format,
    fields: orderedFields,
    includeFolders,
    onlySelected,
    header,
    formatValues,
  };

  const fullText = useMemo(() => {
    if (!snapshot || orderedFields.length === 0) return "";
    return buildExport(rows, snapshot.dir, options, { sizeFormat, dateFormat });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot, rows, orderedFields, format, header, formatValues, sizeFormat, dateFormat]);

  const previewText = useMemo(() => {
    if (!snapshot || orderedFields.length === 0) return "";
    const head = rows.slice(0, PREVIEW_ROWS);
    const text = buildExport(head, snapshot.dir, options, {
      sizeFormat,
      dateFormat,
    });
    if (rows.length > PREVIEW_ROWS) {
      return `${text}\n${t("export.preview.more").replace(
        "{n}",
        String(rows.length - PREVIEW_ROWS),
      )}`;
    }
    return text;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot, rows, orderedFields, format, header, formatValues, sizeFormat, dateFormat, t]);

  const toggleField = (f: ExportField) =>
    setFields((prev) => {
      const next = new Set(prev);
      next.has(f) ? next.delete(f) : next.add(f);
      return next;
    });

  const changeFormat = (f: ExportFormat) => {
    setFormat(f);
    setFileName((n) => withExt(n, FORMAT_EXT[f]));
  };

  const canExport = orderedFields.length > 0 && rows.length > 0;

  const browseFolder = () => {
    openFileBrowser({
      selectMode: "folder",
      title: t("export.folder"),
      initialPath: destDir,
      onPick: (path) => setDestDir(path),
    });
  };

  const copyToClipboard = async () => {
    await writeClipboard(fullText);
    setStatus({ ok: true, text: t("export.copied") });
  };

  const writeTo = async (target: string, overwrite: boolean) => {
    try {
      const written = await writeTextFile(target, fullText, overwrite);
      if (!written) {
        // Datei existiert bereits → vor dem Überschreiben nachfragen.
        useOps.getState().requestConfirm({
          title: t("op.collision.title"),
          message: t("op.collision.exists").replace("{name}", baseName(target)),
          danger: true,
          confirmLabel: t("op.collision.overwrite"),
          icon: FileWarning,
          onConfirm: () => void writeTo(target, true),
        });
        return;
      }
      setStatus({
        ok: true,
        text: t("export.saved").replace("{path}", target),
      });
      // Betroffene Fenster mit dem Zielordner aktualisieren.
      const s = usePanes.getState();
      (["left", "right"] as const).forEach((sd) => {
        if (panelOf(s, sd).path === destDir) void s.refresh(sd);
      });
    } catch (e) {
      setStatus({
        ok: false,
        text: t("export.saveError").replace("{err}", String(e)),
      });
    }
  };

  const save = () => {
    const name = fileName.trim();
    if (!name || !destDir) return;
    void writeTo(joinPath(destDir, name), false);
  };

  return (
    <AppDialog
      dialogKey="export"
      open={!!side}
      onClose={close}
      titleBar={
        <>
          <ListTree size={15} className="text-accent" />
          {t("export.title")}
        </>
      }
      defaultSize={DEFAULT_SIZE}
      minSize={{ w: MIN_W, h: MIN_H }}
    >
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden p-4">
            {/* Formular – kompakt, bricht bei Bedarf um. */}
            <div className="flex flex-wrap items-start gap-x-6 gap-y-4">
              <Section label={t("export.scope")}>
                <div className="flex flex-col gap-1.5">
                  <Radio
                    checked={!onlySelected}
                    onChange={() => setOnlySelected(false)}
                    label={t("export.scope.all").replace(
                      "{n}",
                      String(snapshot?.total ?? 0),
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

              <Section label={t("export.format")}>
                <div className="flex flex-wrap gap-1.5">
                  {FORMATS.map((f) => (
                    <button
                      key={f.value}
                      onClick={() => changeFormat(f.value)}
                      className={cn(
                        "rounded border px-2.5 py-1 text-[12px]",
                        format === f.value
                          ? "border-accent bg-accent text-white"
                          : "border-edge bg-panel text-dim hover:text-text",
                      )}
                    >
                      {t(f.key)}
                    </button>
                  ))}
                </div>
              </Section>

              <Section label={t("export.fields")}>
                <div className="grid grid-cols-2 gap-x-3 gap-y-1.5">
                  {EXPORT_FIELDS.map((f) => (
                    <Check
                      key={f}
                      checked={fields.has(f)}
                      onChange={() => toggleField(f)}
                      label={t(FIELD_LABEL[f])}
                    />
                  ))}
                </div>
              </Section>

              <Section label={t("export.options")}>
                <div className="flex flex-col gap-1.5">
                  <Check
                    checked={includeFolders}
                    onChange={() => setIncludeFolders((v) => !v)}
                    label={t("export.opt.includeFolders")}
                  />
                  <Check
                    checked={formatValues}
                    onChange={() => setFormatValues((v) => !v)}
                    label={t("export.opt.formatValues")}
                  />
                  {(format === "csv" || format === "tsv") && (
                    <Check
                      checked={header}
                      onChange={() => setHeader((v) => !v)}
                      label={t("export.opt.header")}
                    />
                  )}
                </div>
              </Section>
            </div>

            {/* Ziel (Dateiname + Ordner) */}
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
              <label className="flex min-w-[220px] flex-1 items-center gap-2">
                <span className="shrink-0 text-[12px] text-dim">
                  {t("export.filename")}
                </span>
                <input
                  value={fileName}
                  onChange={(e) => setFileName(e.target.value)}
                  spellCheck={false}
                  autoComplete="off"
                  className="min-w-0 flex-1 rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent"
                />
              </label>
              <div className="flex min-w-[260px] flex-1 items-center gap-2">
                <span className="shrink-0 text-[12px] text-dim">
                  {t("export.folder")}
                </span>
                <span className="min-w-0 flex-1 truncate rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-dim">
                  {destDir}
                </span>
                <button
                  onClick={browseFolder}
                  title={t("export.browse")}
                  className="flex shrink-0 items-center gap-1 rounded border border-edge px-2 py-1 text-[12px] text-text hover:border-accent"
                >
                  <FolderSearch size={14} />
                </button>
              </div>
            </div>

            {/* Vorschau (volle Breite, füllt den restlichen Platz). */}
            <div className="flex min-h-0 flex-1 flex-col">
              <span className="mb-1.5 text-[13px] text-text">
                {t("export.preview")}
              </span>
              <textarea
                readOnly
                value={
                  orderedFields.length === 0
                    ? t("export.noFields")
                    : rows.length === 0
                      ? t("export.preview.empty")
                      : previewText
                }
                spellCheck={false}
                className="min-h-0 flex-1 resize-none rounded border border-edge bg-panel-inactive p-2 font-mono text-[11px] leading-relaxed text-text outline-none"
              />
            </div>
          </div>

          {/* Fußzeile */}
          <div className="flex items-center gap-2 border-t border-edge px-4 py-2.5">
            {status && (
              <span
                className={cn(
                  "min-w-0 truncate text-[12px]",
                  status.ok ? "text-dim" : "text-red-400",
                )}
              >
                {status.text}
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
                onClick={copyToClipboard}
                disabled={!canExport}
                className="flex items-center gap-1.5 rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent disabled:opacity-40"
              >
                <ClipboardCopy size={14} />
                {t("export.copy")}
              </button>
              <button
                onClick={save}
                disabled={!canExport || !fileName.trim() || !destDir}
                className="flex items-center gap-1.5 rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
              >
                <Save size={14} />
                {t("export.save")}
              </button>
            </div>
          </div>

    </AppDialog>
  );
}

