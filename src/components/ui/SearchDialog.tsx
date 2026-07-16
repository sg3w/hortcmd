// ============================================================
// Dialog „Suche": durchsucht einen Verzeichnisbaum in vier Modi
// (Dateien nach Name/Inhalt, Duplikate, leere Ordner, große Dateien).
// Die Suche läuft im Backend (`search`) und streamt Treffer; die
// Ergebnisliste ist virtualisiert. Doppelklick zeigt den Treffer im
// aktiven Fenster an.
// ============================================================

import { useCallback, useMemo, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  AlertTriangle,
  FileText,
  Files,
  FolderX,
  HardDrive,
  Loader2,
  Search as SearchIcon,
} from "lucide-react";
import type { SearchHit } from "@/ipc/bindings";
import { search, type SearchMode, type SearchOptions } from "@/ipc/client";
import { useSearchDialog, searchRoot, revealPath } from "@/store/searchStore";
import { useT } from "@/store/settingsStore";
import { formatSize } from "@/lib/format";
import { parentPath } from "@/lib/path";
import type { TransKey } from "@/i18n/dictionaries";
import { Check } from "@/components/ui/dialogControls";
import { cn } from "@/lib/cn";

const ROW_H = 40;

const MODES: { mode: SearchMode; key: TransKey; Icon: typeof Files }[] = [
  { mode: "files", key: "search.mode.files", Icon: FileText },
  { mode: "duplicates", key: "search.mode.duplicates", Icon: Files },
  { mode: "empty_dirs", key: "search.mode.emptyDirs", Icon: FolderX },
  { mode: "large_files", key: "search.mode.largeFiles", Icon: HardDrive },
];

const DEFAULT_IGNORE = "node_modules .git target .cache dist";

export function SearchDialog() {
  const side = useSearchDialog((s) => s.side);
  const close = useSearchDialog((s) => s.close);
  const t = useT();

  const [root, setRoot] = useState("");
  const [mode, setMode] = useState<SearchMode>("files");
  const [name, setName] = useState("");
  const [nameRegex, setNameRegex] = useState(false);
  const [content, setContent] = useState("");
  const [contentRegex, setContentRegex] = useState(false);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [ignore, setIgnore] = useState(DEFAULT_IGNORE);
  const [minSizeMb, setMinSizeMb] = useState(10);

  const [hits, setHits] = useState<SearchHit[]>([]);
  const [running, setRunning] = useState(false);
  const [truncated, setTruncated] = useState(false);
  const [ran, setRan] = useState(false);
  const runToken = useRef(0);

  const scrollRef = useRef<HTMLDivElement>(null);

  // Startordner beim ersten Öffnen übernehmen.
  const open = side !== null;
  useMemo(() => {
    if (open && side) setRoot(searchRoot(side));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const rowVirtualizer = useVirtualizer({
    count: hits.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_H,
    overscan: 15,
  });

  const runSearch = useCallback(async () => {
    if (!root.trim()) return;
    const token = ++runToken.current;
    setHits([]);
    setRunning(true);
    setTruncated(false);
    setRan(true);
    const options: SearchOptions = {
      mode,
      name,
      name_regex: nameRegex,
      content,
      content_regex: contentRegex,
      case_sensitive: caseSensitive,
      ignore_dirs: ignore.split(/[\s,;]+/).map((s) => s.trim()).filter(Boolean),
      min_size: mode === "large_files" ? Math.max(0, minSizeMb) * 1024 * 1024 : 0,
    };
    try {
      const cut = await search(root, options, (batch) => {
        if (token !== runToken.current) return; // veraltete Charge
        setHits((prev) => (prev.length ? [...prev, ...batch] : batch));
      });
      if (token === runToken.current) setTruncated(cut);
    } finally {
      if (token === runToken.current) setRunning(false);
    }
  }, [
    root,
    mode,
    name,
    nameRegex,
    content,
    contentRegex,
    caseSensitive,
    ignore,
    minSizeMb,
  ]);

  const onReveal = (hit: SearchHit) => {
    if (side) void revealPath(side, hit.path);
    close();
  };

  const showFileOpts = mode === "files";
  const showLargeOpts = mode === "large_files";

  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && close()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/50" />
        <Dialog.Content
          aria-describedby={undefined}
          className="fixed left-1/2 top-1/2 z-50 flex h-[86vh] w-[92vw] max-w-[1000px] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-edge bg-panel shadow-2xl"
        >
          <Dialog.Title className="flex items-center gap-2 border-b border-edge bg-header px-4 py-2 text-text">
            <SearchIcon size={16} className="text-dim" aria-hidden />
            <span className="text-[13px] font-medium">{t("search.title")}</span>
          </Dialog.Title>

          {/* Formular */}
          <div className="flex flex-col gap-3 border-b border-edge p-3">
            {/* Modus-Umschalter */}
            <div className="inline-flex flex-wrap gap-1">
              {MODES.map(({ mode: m, key, Icon }) => (
                <button
                  key={m}
                  onClick={() => setMode(m)}
                  className={cn(
                    "flex items-center gap-1.5 rounded border px-2.5 py-1 text-[12px]",
                    mode === m
                      ? "border-accent bg-accent-dim text-text"
                      : "border-edge bg-panel text-dim hover:text-text",
                  )}
                >
                  <Icon size={13} aria-hidden />
                  {t(key)}
                </button>
              ))}
            </div>

            {/* Startordner */}
            <label className="flex items-center gap-2 text-[12px]">
              <span className="w-20 shrink-0 text-dim">{t("search.root")}</span>
              <input
                value={root}
                onChange={(e) => setRoot(e.target.value)}
                className="min-w-0 flex-1 rounded border border-edge bg-panel px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent"
              />
            </label>

            {showFileOpts && (
              <div className="flex flex-col gap-2">
                <label className="flex items-center gap-2 text-[12px]">
                  <span className="w-20 shrink-0 text-dim">
                    {t("search.name")}
                  </span>
                  <input
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder={t("search.name.ph")}
                    onKeyDown={(e) => e.key === "Enter" && runSearch()}
                    className="min-w-0 flex-1 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
                  />
                  <Check
                    checked={nameRegex}
                    onChange={() => setNameRegex((v) => !v)}
                    label={t("search.regex")}
                  />
                </label>
                <label className="flex items-center gap-2 text-[12px]">
                  <span className="w-20 shrink-0 text-dim">
                    {t("search.content")}
                  </span>
                  <input
                    value={content}
                    onChange={(e) => setContent(e.target.value)}
                    placeholder={t("search.content.ph")}
                    onKeyDown={(e) => e.key === "Enter" && runSearch()}
                    className="min-w-0 flex-1 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
                  />
                  <Check
                    checked={contentRegex}
                    onChange={() => setContentRegex((v) => !v)}
                    label={t("search.regex")}
                  />
                </label>
              </div>
            )}

            {showLargeOpts && (
              <label className="flex items-center gap-2 text-[12px]">
                <span className="w-20 shrink-0 text-dim">
                  {t("search.minSize")}
                </span>
                <input
                  type="number"
                  min={0}
                  value={minSizeMb}
                  onChange={(e) =>
                    setMinSizeMb(Math.max(0, Math.floor(Number(e.target.value) || 0)))
                  }
                  className="w-24 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
                />
                <span className="text-dim">MB</span>
              </label>
            )}

            {/* Ignorierte Ordner + Optionen + Start */}
            <div className="flex flex-wrap items-center gap-2">
              <label className="flex min-w-0 flex-1 items-center gap-2 text-[12px]">
                <span className="w-20 shrink-0 text-dim">
                  {t("search.ignore")}
                </span>
                <input
                  value={ignore}
                  onChange={(e) => setIgnore(e.target.value)}
                  className="min-w-0 flex-1 rounded border border-edge bg-panel px-2 py-1 font-mono text-[11px] text-text outline-none focus:border-accent"
                />
              </label>
              {showFileOpts && (
                <Check
                  checked={caseSensitive}
                  onChange={() => setCaseSensitive((v) => !v)}
                  label={t("search.case")}
                />
              )}
              <button
                onClick={runSearch}
                disabled={running || !root.trim()}
                className="flex items-center gap-1.5 rounded bg-accent px-4 py-1.5 text-[13px] text-white hover:brightness-110 disabled:opacity-50"
              >
                {running ? (
                  <Loader2 size={14} className="animate-spin" aria-hidden />
                ) : (
                  <SearchIcon size={14} aria-hidden />
                )}
                {t("search.run")}
              </button>
            </div>
          </div>

          {/* Statuszeile */}
          <div className="flex items-center gap-3 border-b border-edge bg-panel-inactive px-3 py-1 text-[11px] text-dim">
            <span>
              {hits.length} {t("search.results")}
            </span>
            {truncated && (
              <span className="flex items-center gap-1 text-amber-400">
                <AlertTriangle size={12} aria-hidden />
                {t("search.truncated")}
              </span>
            )}
          </div>

          {/* Ergebnisliste */}
          <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
            {ran && !running && hits.length === 0 ? (
              <div className="p-4 text-[13px] text-dim">{t("search.empty")}</div>
            ) : (
              <div
                className="relative w-full"
                style={{ height: rowVirtualizer.getTotalSize() }}
              >
                {rowVirtualizer.getVirtualItems().map((v) => {
                  const h = hits[v.index];
                  return (
                    <div
                      key={v.key}
                      onDoubleClick={() => onReveal(h)}
                      title={h.path}
                      className={cn(
                        "absolute left-0 top-0 flex w-full cursor-default flex-col justify-center border-b border-edge/50 px-3",
                        h.group % 2 === 1 && "bg-accent-dim/30",
                      )}
                      style={{
                        height: ROW_H,
                        transform: `translateY(${v.start}px)`,
                      }}
                    >
                      <div className="flex items-baseline gap-2">
                        <span className="truncate text-[12px] text-text">
                          {h.name}
                        </span>
                        {!h.is_dir && (
                          <span className="shrink-0 font-mono text-[11px] text-dim">
                            {formatSize(h.size)}
                          </span>
                        )}
                        {h.detail && (
                          <span className="truncate font-mono text-[11px] text-emerald-400">
                            {h.detail}
                          </span>
                        )}
                      </div>
                      <span className="truncate text-[10px] text-dim">
                        {parentPath(h.path)}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
