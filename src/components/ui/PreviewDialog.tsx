// ============================================================
// Datei-Vorschau (F3). Modi je Format:
//   Bild (+ EXIF) · Markdown gerendert · Syntaxhighlighting ·
//   CSV-Tabelle · Klartext · Hex – umschaltbar in der Kopfzeile.
// ============================================================

import { useEffect, useMemo, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { FileText, Image as ImageIcon, Binary, Info, X } from "lucide-react";
import { useOps } from "@/store/opsStore";
import { useT } from "@/store/settingsStore";
import type { TransKey } from "@/i18n/dictionaries";
import { formatSize } from "@/lib/format";
import {
  extNameOf,
  highlightCode,
  parseCsv,
  renderMarkdown,
  viewModesFor,
  type PreviewMode,
} from "@/lib/preview";
import { cn } from "@/lib/cn";

export function PreviewDialog() {
  const preview = useOps((s) => s.preview);
  const setPreview = useOps((s) => s.setPreview);
  const t = useT();

  const isImage = preview?.kind === "image";
  const hasHex = preview?.hex != null;
  const { modes, def } = useMemo(
    () => viewModesFor(preview?.name ?? "", hasHex),
    [preview?.name, hasHex],
  );

  const [mode, setMode] = useState<PreviewMode>(def);
  const [showExif, setShowExif] = useState(false);

  // Bei Dateiwechsel Modus auf den Standard zurücksetzen.
  useEffect(() => {
    setMode(def);
    setShowExif(false);
  }, [preview?.name, def]);

  const KindIcon = isImage ? ImageIcon : hasHex ? Binary : FileText;
  const ext = preview ? extNameOf(preview.name) : "";
  const text = preview?.text ?? "";

  return (
    // Dialog.Root bleibt gemountet und wird nur über `open` gesteuert.
    // Ein Unmount im offenen Zustand (früher `if (!preview) return null`)
    // ließe Radix `pointer-events: none` auf <body> zurück → App reagiert
    // nach dem Schließen nicht mehr.
    <Dialog.Root open={!!preview} onOpenChange={(o) => !o && setPreview(null)}>
      {preview && (
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[64] bg-black/60" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-[65] flex h-[80vh] w-[72vw] max-w-[1000px] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-edge bg-panel shadow-2xl">
          <Dialog.Title className="flex items-center gap-2 border-b border-edge bg-header px-4 py-2 text-text">
            <KindIcon size={15} className="shrink-0 text-accent" />
            <span className="truncate font-mono text-[13px]">{preview.name}</span>
            <span className="ml-2 shrink-0 text-[11px] text-dim">
              {formatSize(preview.size)}
            </span>

            {/* Modus-Umschalter (Text-Formate) bzw. EXIF-Schalter (Bild) */}
            {!isImage && modes.length > 1 && (
              <div className="ml-auto inline-flex overflow-hidden rounded border border-edge">
                {modes.map((m) => (
                  <button
                    key={m}
                    onClick={() => setMode(m)}
                    className={cn(
                      "px-2.5 py-0.5 text-[12px]",
                      mode === m
                        ? "bg-accent text-white"
                        : "bg-panel text-dim hover:text-text",
                    )}
                  >
                    {t(`preview.mode.${m}` as TransKey)}
                  </button>
                ))}
              </div>
            )}
            {isImage && (
              <button
                onClick={() => setShowExif((v) => !v)}
                className={cn(
                  "ml-auto flex items-center gap-1 rounded border border-edge px-2 py-0.5 text-[12px]",
                  showExif
                    ? "bg-accent text-white"
                    : "bg-panel text-dim hover:text-text",
                )}
              >
                <Info size={12} />
                {t("preview.exif")}
              </button>
            )}

            <Dialog.Close className="ml-2 shrink-0 rounded p-1 text-dim hover:bg-accent-dim hover:text-text">
              <X size={15} />
            </Dialog.Close>
          </Dialog.Title>

          <div className="flex min-h-0 flex-1">
            <div className="min-h-0 flex-1 overflow-auto bg-bg">
              {isImage ? (
                <div className="flex h-full items-center justify-center p-4">
                  <img
                    src={preview.data_url ?? ""}
                    alt={preview.name}
                    className="max-h-full max-w-full object-contain"
                  />
                </div>
              ) : (
                <PreviewBody mode={mode} text={text} hex={preview.hex} ext={ext} name={preview.name} />
              )}
            </div>

            {/* EXIF-Panel */}
            {isImage && showExif && (
              <div className="w-[280px] shrink-0 overflow-y-auto border-l border-edge bg-panel-inactive p-3">
                {preview.exif.length === 0 ? (
                  <p className="text-[12px] text-dim">{t("preview.exif.none")}</p>
                ) : (
                  <dl className="flex flex-col gap-1.5">
                    {preview.exif.map((tag) => (
                      <div key={tag.name} className="flex flex-col">
                        <dt className="text-[10px] uppercase tracking-wide text-dim">
                          {tag.name}
                        </dt>
                        <dd className="break-words font-mono text-[12px] text-text">
                          {tag.value}
                        </dd>
                      </div>
                    ))}
                  </dl>
                )}
              </div>
            )}
          </div>

          {preview.truncated && (
            <div className="flex-shrink-0 border-t border-edge bg-header px-4 py-1.5 text-[11px] text-dim">
              {t("preview.truncated")}
            </div>
          )}
        </Dialog.Content>
      </Dialog.Portal>
      )}
    </Dialog.Root>
  );
}

function PreviewBody({
  mode,
  text,
  hex,
  ext,
  name,
}: {
  mode: PreviewMode;
  text: string;
  hex: string | null;
  ext: string;
  name: string;
}) {
  if (mode === "rendered") {
    return (
      <div
        className="md-body px-4 py-3"
        dangerouslySetInnerHTML={{ __html: renderMarkdown(text) }}
      />
    );
  }
  if (mode === "highlight") {
    return (
      <pre className="m-0 p-3 font-mono text-[12px] leading-[1.5]">
        <code
          dangerouslySetInnerHTML={{ __html: highlightCode(text, ext) }}
        />
      </pre>
    );
  }
  if (mode === "table") {
    const rows = parseCsv(text, name);
    return (
      <div className="overflow-auto p-3">
        <table className="border-collapse font-mono text-[12px]">
          <tbody>
            {rows.map((row, r) => (
              <tr key={r}>
                {row.map((cell, c) => (
                  <td
                    key={c}
                    className={cn(
                      "border border-edge px-2 py-1 text-text",
                      r === 0 && "bg-header font-semibold",
                    )}
                  >
                    {cell}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }
  // raw / text / hex → Klartext bzw. Hex-Dump.
  const content = mode === "hex" ? (hex ?? "") : text;
  return (
    <pre
      className={cn(
        "m-0 p-3 font-mono text-[12px] leading-[1.5] text-text",
        mode === "hex" ? "whitespace-pre" : "whitespace-pre-wrap break-words",
      )}
    >
      {content}
    </pre>
  );
}
