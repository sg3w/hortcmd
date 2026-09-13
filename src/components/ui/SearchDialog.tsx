// ============================================================
// "Search" dialog: searches a directory tree in four modes (files by
// name/content, duplicates, empty folders, large files). The search
// runs in the backend (`search`) and streams hits; the result list is
// virtualized. A double-click reveals the hit in the active window.
// ============================================================

import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {
  AlertTriangle,
  FileText,
  Files,
  FolderX,
  HardDrive,
  Loader2,
  Search as SearchIcon,
  XCircle,
} from "lucide-react";
import type { SearchHit, SearchStatus } from "@/ipc/bindings";
import { explain } from "@/features/commander/guard";
import { loadCaps } from "@/store/capsStore";
import {
  cancelSearch,
  search,
  type SearchMode,
  type SearchOptions,
} from "@/ipc/client";
import { useSearchDialog, searchRoot, revealPath } from "@/store/searchStore";
import { describeErrorFull } from "@/store/errorText";
import { useT } from "@/store/settingsStore";
import type { TransKey } from "@/i18n/dictionaries";
import { Check } from "@/components/ui/dialogControls";
import { cn } from "@/lib/cn";
import { consumeDialogKey, markDialogKey } from "@/lib/keys";

import { SearchResults } from "./SearchResults";
import { useDialogFocusReturn } from "./useDialogFocusReturn";

const MODES: { mode: SearchMode; key: TransKey; Icon: typeof Files }[] = [
  { mode: "files", key: "search.mode.files", Icon: FileText },
  { mode: "duplicates", key: "search.mode.duplicates", Icon: Files },
  { mode: "empty_dirs", key: "search.mode.emptyDirs", Icon: FolderX },
  { mode: "large_files", key: "search.mode.largeFiles", Icon: HardDrive },
];

const DEFAULT_IGNORE = "node_modules .git target .cache dist";

// Each run gets its own token, so a cancel can never hit a later run.
let nextToken = 0;

/**
 * Warning shown for a result that is not the full answer. `complete`
 * needs none; `partial` only says that unreadable entries were skipped.
 */
/** Why `root` cannot be searched, or null. The traversal is the local
 *  walker, so a remote root has no search capability yet (TICKET-031). */
async function explainRoot(root: string): Promise<string | null> {
  await loadCaps(root);
  return explain(root, "search");
}

const STATUS_WARNING: Partial<Record<SearchStatus, TransKey>> = {
  truncated: "search.truncated",
  limited: "search.limited",
  partial: "search.partial",
  cancelled: "search.cancelled",
};

export function SearchDialog() {
  const returnFocus = useDialogFocusReturn();
  const fieldId = useId();
  const side = useSearchDialog((s) => s.side);
  const close = useSearchDialog((s) => s.close);
  // Mode the caller asked for — the Befehle menu opens the dialog straight
  // into "duplicates", "empty folders" or "large files" (TICKET-058).
  const requestedMode = useSearchDialog((s) => s.mode);
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
  const [status, setStatus] = useState<SearchStatus | null>(null);
  /** Why the chosen root cannot be searched, or null (TICKET-031). The
   *  root is a free-text field, so the answer is re-checked per run
   *  rather than taken from the pane the dialog was opened over. */
  const [failure, setFailure] = useState<string | null>(null);
  const [blocked, setBlocked] = useState<string | null>(null);
  const [skipped, setSkipped] = useState(0);
  const [ran, setRan] = useState(false);
  const runToken = useRef(0);
  // Token of the run currently in the backend (null = nothing running).
  const activeToken = useRef<string | null>(null);
  const startedToken = useRef<string | null>(null);


  // Adopt the start folder and the requested mode on first open.
  const open = side !== null;
  useMemo(() => {
    if (open && side) {
      setRoot(searchRoot(side));
      setMode(requestedMode);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  /** Stops the run in the backend (new search, dialog closed, Stop). */
  const stopRunning = useCallback(() => {
    const token = activeToken.current;
    if (!token) return;
    const cancelledRun = ++runToken.current;
    activeToken.current = null;
    setRunning(false);
    setStatus("cancelled");
    if (startedToken.current === token) {
      startedToken.current = null;
      void cancelSearch(token).catch(error => {
        if (runToken.current === cancelledRun) setFailure(describeErrorFull(error));
      });
    }
  }, []);

  // A closed dialog must not leave a scan running.
  useEffect(() => {
    if (!open) stopRunning();
  }, [open, stopRunning]);

  useEffect(() => () => stopRunning(), [stopRunning]);

  const runSearch = useCallback(async () => {
    if (!root.trim()) return;
    stopRunning();
    const run = ++runToken.current;
    const token = `search:${++nextToken}`;
    activeToken.current = token;
    setHits([]);
    setRunning(true);
    setStatus(null);
    setSkipped(0);
    setRan(true);
    setBlocked(null);
    setFailure(null);
    const reason = await explainRoot(root);
    if (run !== runToken.current || activeToken.current !== token) return;
    if (reason) {
      setBlocked(reason);
      setRunning(false);
      if (activeToken.current === token) activeToken.current = null;
      return;
    }
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
      startedToken.current = token;
      const summary = await search(root, options, token, (batch) => {
        if (run !== runToken.current) return; // stale batch
        setHits((prev) => (prev.length ? [...prev, ...batch] : batch));
      });
      if (run === runToken.current) {
        setStatus(summary.status);
        setSkipped(summary.unreadable);
      }
    } catch (error) {
      if (run === runToken.current) setFailure(describeErrorFull(error));
    } finally {
      if (startedToken.current === token) startedToken.current = null;
      if (activeToken.current === token) activeToken.current = null;
      if (run === runToken.current) setRunning(false);
    }
  }, [
    stopRunning,
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

  const warning = status ? STATUS_WARNING[status] : undefined;
  // Keep streamed result counts silent until the run settles. This region stays
  // mounted so repeated runs are announced after the intervening running state.
  let announcement = "";
  if (blocked) announcement = blocked;
  else if (running) announcement = t("search.running");
  else if (status) {
    announcement = warning
      ? `${t(warning)}: ${hits.length} ${t("search.results")}`
      : t("search.complete").replace("{n}", String(hits.length));
    if (status === "partial") announcement += ` (${skipped})`;
  }

  const showFileOpts = mode === "files";
  const showLargeOpts = mode === "large_files";

  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && close()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/50" />
        <Dialog.Content
          onCloseAutoFocus={returnFocus}
          onKeyDownCapture={markDialogKey}
          aria-describedby={undefined}
          className="fixed left-1/2 top-1/2 z-50 flex h-[86vh] w-[92vw] max-w-[1000px] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-edge bg-panel shadow-2xl"
        >
          <Dialog.Title className="flex shrink-0 items-center gap-2 border-b border-edge bg-header px-4 py-2 text-text">
            <SearchIcon size={16} className="text-dim" aria-hidden />
            <span className="text-[13px] font-medium">{t("search.title")}</span>
          </Dialog.Title>

          {/* Form */}
          <div className="flex min-h-0 flex-col gap-3 overflow-y-auto border-b border-edge p-3">
            {/* Mode switch */}
            <div className="inline-flex flex-wrap gap-1">
              {MODES.map(({ mode: m, key, Icon }) => (
                <button
                  key={m}
                  aria-pressed={mode === m}
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

            {/* Start folder */}
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
                <div className="flex items-center gap-2 text-[12px]">
                  <label htmlFor={`${fieldId}-name`} className="w-20 shrink-0 text-dim">
                    {t("search.name")}
                  </label>
                  <input
                    id={`${fieldId}-name`}
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder={t("search.name.ph")}
                    onKeyDown={(e) => {
                      if (e.key !== "Enter") return;
                      consumeDialogKey(e);
                      void runSearch();
                    }}
                    className="min-w-0 flex-1 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
                  />
                  <Check
                    checked={nameRegex}
                    onChange={() => setNameRegex((v) => !v)}
                    label={t("search.regex")}
                  />
                </div>
                <div className="flex items-center gap-2 text-[12px]">
                  <label htmlFor={`${fieldId}-content`} className="w-20 shrink-0 text-dim">
                    {t("search.content")}
                  </label>
                  <input
                    id={`${fieldId}-content`}
                    value={content}
                    onChange={(e) => setContent(e.target.value)}
                    placeholder={t("search.content.ph")}
                    onKeyDown={(e) => {
                      if (e.key !== "Enter") return;
                      consumeDialogKey(e);
                      void runSearch();
                    }}
                    className="min-w-0 flex-1 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
                  />
                  <Check
                    checked={contentRegex}
                    onChange={() => setContentRegex((v) => !v)}
                    label={t("search.regex")}
                  />
                </div>
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

            {/* Ignored folders + options + start */}
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
              {running ? (
                <button
                  onClick={stopRunning}
                  className="flex items-center gap-1.5 rounded border border-edge bg-panel px-4 py-1.5 text-[13px] text-text hover:border-accent"
                >
                  <Loader2 size={14} className="animate-spin" aria-hidden />
                  {t("search.stop")}
                </button>
              ) : (
                <button
                  onClick={runSearch}
                  disabled={!root.trim()}
                  className="flex items-center gap-1.5 rounded bg-accent px-4 py-1.5 text-[13px] text-white hover:brightness-110 disabled:opacity-50"
                >
                  <SearchIcon size={14} aria-hidden />
                  {t("search.run")}
                </button>
              )}
            </div>
          </div>

          <div role="status" aria-atomic="true" className="sr-only">{announcement}</div>

          {failure && <div role="alert" className="px-3 py-2 text-[12px] text-red-400">{failure}</div>}

          {/* Status line */}
          <div className="flex shrink-0 items-center gap-3 border-b border-edge bg-panel-inactive px-3 py-1 text-[11px] text-dim">
            <span>
              {hits.length} {t("search.results")}
            </span>
            {blocked && <span className="text-amber-400">{blocked}</span>}
            {warning && (
              <span
                className={cn(
                  "flex items-center gap-1",
                  status === "cancelled" ? "text-dim" : "text-amber-400",
                )}
              >
                {status === "cancelled" ? (
                  <XCircle size={12} aria-hidden />
                ) : (
                  <AlertTriangle size={12} aria-hidden />
                )}
                {t(warning)}
                {status === "partial" && ` (${skipped})`}
              </span>
            )}
          </div>

          {/* Result list */}
          {ran && !running && !failure && hits.length === 0 ? (
            <div className="p-4 text-[13px] text-dim">{t("search.empty")}</div>
          ) : (
            <SearchResults key={runToken.current} hits={hits} onReveal={onReveal} />
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
