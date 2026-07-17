// ============================================================
// "Compare & synchronize directories" dialog: compares the left against
// the right window (recursively, by size + modification time), shows the
// differences as a list and copies selected files left↔right. Comparing/
// copying is handled by the backend.
//
// Layout: form on top, list below (full width), window resizable.
// ============================================================

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  AlertTriangle,
  ChevronsLeft,
  ChevronsRight,
  Equal,
  EqualNot,
  FolderSync,
  RefreshCw,
  type LucideIcon,
} from "lucide-react";
import type { DiffEntry } from "@/ipc/bindings";
import { panelOf, usePanes } from "@/store/panesStore";
import { useCompareDialog } from "@/store/compareStore";
import { useOps } from "@/store/opsStore";
import { useSettings, useT } from "@/store/settingsStore";
import { compareDirs, syncCopy } from "@/ipc/client";
import { joinPath } from "@/lib/path";
import { formatDate, formatSize } from "@/lib/format";
import type { TransKey } from "@/i18n/dictionaries";
import { Check } from "@/components/ui/dialogControls";
import { cn } from "@/lib/cn";
import { AppDialog } from "@/components/ui/AppDialog";

const DEFAULT_SIZE = { w: 900, h: 620 };
const MIN_W = 640;
const MIN_H = 440;
/** Fixed row height (px) of the virtualized comparison list. */
const ROW_H = 22;

type Filter = "same" | "diff" | "leftOnly" | "rightOnly";

/** Maps a status to its filter category. */
function category(status: string): Filter {
  if (status === "same") return "same";
  if (status === "left_only") return "leftOnly";
  if (status === "right_only") return "rightOnly";
  return "diff";
}

interface StatusMeta {
  Icon: LucideIcon;
  color: string;
  key: TransKey;
}

const STATUS_META: Record<string, StatusMeta> = {
  same: { Icon: Equal, color: "text-dim", key: "compare.status.same" },
  newer_left: {
    Icon: ChevronsRight,
    color: "text-accent",
    key: "compare.status.newer_left",
  },
  newer_right: {
    Icon: ChevronsLeft,
    color: "text-accent",
    key: "compare.status.newer_right",
  },
  different: {
    Icon: EqualNot,
    color: "text-amber-400",
    key: "compare.status.different",
  },
  left_only: {
    Icon: ChevronsRight,
    color: "text-emerald-400",
    key: "compare.status.left_only",
  },
  right_only: {
    Icon: ChevronsLeft,
    color: "text-emerald-400",
    key: "compare.status.right_only",
  },
};

export function CompareDialog() {
  const open = useCompareDialog((s) => s.open);
  const setOpen = useCompareDialog((s) => s.setOpen);
  const t = useT();
  const close = () => setOpen(false);
  const sizeFormat = useSettings((s) => s.sizeFormat);
  const dateFormat = useSettings((s) => s.dateFormat);

  const [left, setLeft] = useState("");
  const [right, setRight] = useState("");
  const [entries, setEntries] = useState<DiffEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [recursive, setRecursive] = useState(false);
  const [truncated, setTruncated] = useState(false);
  const [filters, setFilters] = useState<Record<Filter, boolean>>({
    same: false,
    diff: true,
    leftOnly: true,
    rightOnly: true,
  });
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(
    null,
  );

  // Every run gets a token; late batches of a superseded run (e.g. through
  // re-comparing/filter change) are discarded.
  const runToken = useRef(0);

  const run = useCallback(async (l: string, r: string, rec: boolean) => {
    const token = ++runToken.current;
    setLoading(true);
    setError(null);
    setTruncated(false);
    setSelected(new Set());
    setEntries([]);
    try {
      const cut = await compareDirs(l, r, rec, (batch) => {
        if (runToken.current !== token) return; // stale run
        setEntries((prev) => [...prev, ...batch]);
      });
      if (runToken.current === token) setTruncated(cut);
    } catch (e) {
      if (runToken.current === token) {
        setEntries([]);
        setError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      if (runToken.current === token) setLoading(false);
    }
  }, []);

  // On open, adopt the paths of both windows and compare.
  useEffect(() => {
    if (!open) return;
    const l = panelOf(usePanes.getState(), "left").path;
    const r = panelOf(usePanes.getState(), "right").path;
    setLeft(l);
    setRight(r);
    // Recursive deliberately always starts disabled (TICKET-007) — a
    // previous state must not be adopted automatically.
    setRecursive(false);
    setStatus(null);
    void run(l, r, false);
  }, [open, run]);

  const toggleRecursive = () => {
    const next = !recursive;
    setRecursive(next);
    void run(left, right, next);
  };

  const counts = useMemo(() => {
    const c = { total: entries.length, same: 0, diff: 0, leftOnly: 0, rightOnly: 0 };
    for (const e of entries) c[category(e.status)] += 1;
    return c;
  }, [entries]);

  const visible = useMemo(
    () => entries.filter((e) => filters[category(e.status)]),
    [entries, filters],
  );

  // Virtualized list: only the rows visible in the window are rendered.
  const scrollRef = useRef<HTMLDivElement>(null);
  const rowVirtualizer = useVirtualizer({
    count: visible.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_H,
    overscan: 12,
  });

  const allVisibleSelected =
    visible.length > 0 && visible.every((e) => selected.has(e.rel));

  const toggleRow = (rel: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(rel) ? next.delete(rel) : next.add(rel);
      return next;
    });

  const toggleAll = () =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (allVisibleSelected) visible.forEach((e) => next.delete(e.rel));
      else visible.forEach((e) => next.add(e.rel));
      return next;
    });

  const toggleFilter = (f: Filter) =>
    setFilters((prev) => ({ ...prev, [f]: !prev[f] }));

  // Number of copyable selected items per direction (source must exist).
  const selCount = useMemo(() => {
    let toRight = 0;
    let toLeft = 0;
    for (const e of entries) {
      if (!selected.has(e.rel)) continue;
      if (e.left) toRight += 1;
      if (e.right) toLeft += 1;
    }
    return { toRight, toLeft };
  }, [entries, selected]);

  const copy = async (dir: "toRight" | "toLeft") => {
    const items: [string, string][] = [];
    for (const e of entries) {
      if (!selected.has(e.rel)) continue;
      if (dir === "toRight" && e.left) {
        items.push([joinPath(left, e.rel), joinPath(right, e.rel)]);
      } else if (dir === "toLeft" && e.right) {
        items.push([joinPath(right, e.rel), joinPath(left, e.rel)]);
      }
    }
    if (!items.length) return;

    const res = await syncCopy(items);

    // Refresh the affected windows and compare again.
    const s = usePanes.getState();
    (["left", "right"] as const).forEach((sd) => {
      const p = panelOf(s, sd).path;
      if (p === left || p === right) void s.refresh(sd);
    });
    await run(left, right, recursive);

    if (res.errors.length) {
      setStatus({ ok: false, text: t("op.errors") });
      useOps.getState().requestConfirm({
        title: t("op.errors"),
        message: res.errors.slice(0, 12).join("\n"),
        onConfirm: () => {},
      });
    } else {
      setStatus({
        ok: true,
        text: t("compare.copied").replace("{n}", String(res.ok)),
      });
    }
  };

  const FILTER_ROW: { key: Filter; label: TransKey; count: number }[] = [
    { key: "diff", label: "compare.filter.diff", count: counts.diff },
    { key: "leftOnly", label: "compare.filter.leftOnly", count: counts.leftOnly },
    { key: "rightOnly", label: "compare.filter.rightOnly", count: counts.rightOnly },
    { key: "same", label: "compare.filter.same", count: counts.same },
  ];

  const footer = (
    <div className="flex items-center gap-3 border-t border-edge px-4 py-2.5">
      <span className="text-[12px] text-dim">
        {t("compare.summary")
          .replace("{total}", String(counts.total))
          .replace("{diff}", String(counts.diff))
          .replace("{left}", String(counts.leftOnly))
          .replace("{right}", String(counts.rightOnly))}
      </span>
      {status && (
        <span
          className={cn("text-[12px]", status.ok ? "text-dim" : "text-red-400")}
        >
          {status.text}
        </span>
      )}
      <div className="ml-auto flex gap-2">
        <button
          onClick={() => void copy("toLeft")}
          disabled={selCount.toLeft === 0}
          className="flex items-center gap-1.5 rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent disabled:opacity-40"
        >
          <ChevronsLeft size={14} />
          {t("compare.toLeft")}
          {selCount.toLeft > 0 && ` (${selCount.toLeft})`}
        </button>
        <button
          onClick={() => void copy("toRight")}
          disabled={selCount.toRight === 0}
          className="flex items-center gap-1.5 rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent disabled:opacity-40"
        >
          {t("compare.toRight")}
          {selCount.toRight > 0 && ` (${selCount.toRight})`}
          <ChevronsRight size={14} />
        </button>
        <button
          onClick={close}
          className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110"
        >
          {t("op.close")}
        </button>
      </div>
    </div>
  );

  return (
    <AppDialog
      dialogKey="compare"
      open={open}
      onClose={close}
      titleBar={
        <>
          <FolderSync size={15} className="text-accent" />
          {t("compare.title")}
        </>
      }
      footer={footer}
      defaultSize={DEFAULT_SIZE}
      minSize={{ w: MIN_W, h: MIN_H }}
    >
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden p-4">
            {/* Form: paths + refresh + filters */}
            <div className="flex flex-col gap-2">
              <div className="flex items-center gap-2 text-[12px]">
                <span className="shrink-0 font-medium text-dim">
                  {t("compare.left")}
                </span>
                <span className="min-w-0 flex-1 truncate rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-text">
                  {left}
                </span>
                <span className="shrink-0 font-medium text-dim">
                  {t("compare.right")}
                </span>
                <span className="min-w-0 flex-1 truncate rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-text">
                  {right}
                </span>
                <button
                  onClick={() => void run(left, right, recursive)}
                  title={t("compare.refresh")}
                  className="flex shrink-0 items-center gap-1.5 rounded border border-edge px-2 py-1 text-text hover:border-accent"
                >
                  <RefreshCw size={14} className={cn(loading && "animate-spin")} />
                </button>
              </div>

              <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
                <Check
                  checked={recursive}
                  onChange={toggleRecursive}
                  label={t("compare.recursive")}
                />
                <span className="h-4 w-px bg-edge" aria-hidden />
                {FILTER_ROW.map((f) => (
                  <Check
                    key={f.key}
                    checked={filters[f.key]}
                    onChange={() => toggleFilter(f.key)}
                    label={`${t(f.label)} (${f.count})`}
                  />
                ))}
              </div>
            </div>

            {/* Warning when the result was truncated (limit reached). */}
            {truncated && (
              <div className="flex items-center gap-2 rounded border border-amber-500/50 bg-amber-500/10 px-3 py-1.5 text-[12px] text-amber-400">
                <AlertTriangle size={14} className="shrink-0" />
                <span>{t("compare.truncated")}</span>
              </div>
            )}

            {/* List (virtualized – only visible rows in the DOM) */}
            <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden rounded border border-edge bg-panel-inactive">
              {/* Header row */}
              <div className="flex items-center gap-2 border-b border-edge px-2 py-1 text-[11px] font-medium text-dim">
                <input
                  type="checkbox"
                  checked={allVisibleSelected}
                  onChange={toggleAll}
                  disabled={visible.length === 0}
                  className="accent-[color:var(--accent)]"
                  title={t("compare.selectAll")}
                />
                <span className="min-w-0 flex-1">{t("compare.col.path")}</span>
                <span className="w-20 text-right">{t("compare.left")}</span>
                <span className="w-8 text-center" />
                <span className="w-20 text-right">{t("compare.right")}</span>
              </div>

              <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
                {error ? (
                  <div className="p-3 text-[12px] text-red-400">
                    {t("compare.error").replace("{err}", error)}
                  </div>
                ) : entries.length === 0 ? (
                  <div className="p-3 text-[12px] text-dim">
                    {loading ? t("compare.loading") : t("compare.identical")}
                  </div>
                ) : visible.length === 0 ? (
                  <div className="p-3 text-[12px] text-dim">
                    {loading ? t("compare.loading") : t("compare.empty")}
                  </div>
                ) : (
                  <div
                    style={{
                      height: rowVirtualizer.getTotalSize(),
                      position: "relative",
                    }}
                  >
                    {rowVirtualizer.getVirtualItems().map((vItem) => {
                      const e = visible[vItem.index];
                      const meta = STATUS_META[e.status] ?? STATUS_META.different;
                      return (
                        <label
                          key={e.rel}
                          className="absolute inset-x-0 flex cursor-pointer items-center gap-2 border-b border-edge/40 px-2 text-[11px] hover:bg-accent-dim"
                          style={{
                            height: ROW_H,
                            transform: `translateY(${vItem.start}px)`,
                          }}
                        >
                          <input
                            type="checkbox"
                            checked={selected.has(e.rel)}
                            onChange={() => toggleRow(e.rel)}
                            className="accent-[color:var(--accent)]"
                          />
                          <span className="min-w-0 flex-1 truncate font-mono text-text">
                            {e.rel}
                          </span>
                          <span className="w-20 text-right font-mono text-dim">
                            {e.left ? formatSize(e.left_size, sizeFormat) : "—"}
                          </span>
                          <span
                            className={cn("flex w-8 justify-center", meta.color)}
                            title={`${t(meta.key)} · ${
                              e.left
                                ? formatDate(e.left_modified, dateFormat)
                                : "—"
                            } / ${
                              e.right
                                ? formatDate(e.right_modified, dateFormat)
                                : "—"
                            }`}
                          >
                            <meta.Icon size={14} />
                          </span>
                          <span className="w-20 text-right font-mono text-dim">
                            {e.right ? formatSize(e.right_size, sizeFormat) : "—"}
                          </span>
                        </label>
                      );
                    })}
                  </div>
                )}
              </div>

              {/* Streaming hint while reading is still in progress. */}
              {loading && entries.length > 0 && (
                <div className="pointer-events-none absolute bottom-1 right-2 rounded bg-panel/90 px-2 py-0.5 text-[11px] text-dim shadow">
                  {t("compare.loading")}
                </div>
              )}
            </div>
          </div>

    </AppDialog>
  );
}
