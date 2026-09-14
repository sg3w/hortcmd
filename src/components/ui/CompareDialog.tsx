// ============================================================
// "Compare & synchronize directories" dialog: compares the left against
// the right window (recursively, by size + modification time), shows the
// differences as a list and copies selected files left↔right. Comparing/
// copying is handled by the backend.
//
// Layout: form on top, list below (full width), window resizable.
// ============================================================

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CompareResults } from "./CompareResults";
import { locationIdentity, parseLocation } from "@/lib/location";
import {
  AlertTriangle,
  ChevronsLeft,
  ChevronsRight,
  FolderSync,
  RefreshCw,
} from "lucide-react";
import type { DiffEntry } from "@/ipc/bindings";
import { panelOf, usePanes } from "@/store/panesStore";
import { useCompareDialog } from "@/store/compareStore";
import { describeErrorFull, describeItemErrors } from "@/store/errorText";
import { useOps } from "@/store/opsStore";
import { translate, useT } from "@/store/settingsStore";
import { compareDirs, syncCopy } from "@/ipc/client";
import { useCaps } from "@/store/capsStore";
import { explainPair, pairState } from "@/features/commander/guard";
import { joinPath } from "@/lib/path";

import type { TransKey } from "@/i18n/dictionaries";
import { Check } from "@/components/ui/dialogControls";
import { cn } from "@/lib/cn";
import { AppDialog } from "@/components/ui/AppDialog";

const DEFAULT_SIZE = { w: 1100, h: 660 };
const MIN_W = 320;
const MIN_H = 280;
/** Fixed row height (px) of the virtualized comparison list. */


type Filter = "same" | "diff" | "leftOnly" | "rightOnly";

/** Maps a status to its filter category. */
function category(status: string): Filter {
  if (status === "same") return "same";
  if (status === "left_only") return "leftOnly";
  if (status === "right_only") return "rightOnly";
  return "diff";
}

export function CompareDialog() {
  const open = useCompareDialog((s) => s.open);
  const setOpen = useCompareDialog((s) => s.setOpen);
  const t = useT();
  useCaps(s=>s.known);
  const close = () => { ++runToken.current; setOpen(false); };

  const [left, setLeft] = useState("");
  const [right, setRight] = useState("");
  const [entries, setEntries] = useState<DiffEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [copying, setCopying] = useState(false);
  const copyingRef = useRef(false);
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
    setStatus(null);
    setSelected(new Set());
    setEntries([]);
    // Both folders are edited here independently, so each is asked for
    // its own capability before the backend is called (TICKET-031) — an
    // allowed left side is no licence for the right one.
    if (panelOf(usePanes.getState(),"left").archive || panelOf(usePanes.getState(),"right").archive) {
      setError(translate("compare.archiveUnsupported"));setLoading(false);return;
    }
    const blocked = await explainPair(l, "compareDirs", r, "compareDirs");
    if (runToken.current !== token) return;
    if (blocked) {
      if (runToken.current === token) {
        setError(blocked);
        setLoading(false);
      }
      return;
    }
    try {
      const cut = await compareDirs(l, r, rec, (batch) => {
        if (runToken.current !== token) return; // stale run
        setEntries((prev) => [...prev, ...batch.map(e=>({...e,status:(e.left&&e.left_modified===0)||(e.right&&e.right_modified===0)?"unknown":e.status}))]);
      });
      if (runToken.current === token) setTruncated(cut);
    } catch (e) {
      if (runToken.current === token) {
        setEntries([]);
        setSelected(new Set());
        setError(describeErrorFull(e));
      }
    } finally {
      if (runToken.current === token) setLoading(false);
    }
  }, []);

  // On open, adopt the paths of both windows and compare.
  useEffect(() => {
    if (!open) { ++runToken.current; return; }
    const lp=panelOf(usePanes.getState(), "left"), rp=panelOf(usePanes.getState(), "right");
    const l=lp.path, r=rp.path;
    setLeft(l);
    setRight(r);
    // Recursive deliberately always starts disabled (TICKET-007) — a
    // previous state must not be adopted automatically.
    setRecursive(false);
    setStatus(null);
    void run(l, r, false);
    return () => { ++runToken.current; };
  }, [open, run]);

  const toggleRecursive = () => {
    if (copyingRef.current) return;
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
    () => entries.filter((e) => filters[category(e.status)]).sort((a,b)=>a.rel.localeCompare(b.rel)),
    [entries, filters],
  );

  const allVisibleSelected =
    visible.length > 0 && visible.every((e) => selected.has(e.rel));

  const toggleRow = (rel: string) => {
    if (copyingRef.current) return;
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(rel) ? next.delete(rel) : next.add(rel);
      return next;
    });
  };

  const toggleAll = () => {
    if (copyingRef.current) return;
    setSelected((prev) => {
      const next = new Set(prev);
      if (allVisibleSelected) visible.forEach((e) => next.delete(e.rel));
      else visible.forEach((e) => next.add(e.rel));
      return next;
    });
  };

  const toggleFilter = (f: Filter) => {
    if (copyingRef.current) return;
    if (filters[f]) {
      const retained=new Set(entries.filter(e=>category(e.status)!==f).map(e=>e.rel));
      setSelected(prev=>new Set([...prev].filter(rel=>retained.has(rel))));
    }
    setFilters(prev=>({...prev,[f]:!prev[f]}));
  };

  // Number of copyable selected items per direction (source must exist).
  const selCount = useMemo(() => {
    let toRight = 0;
    let toLeft = 0;
    for (const e of visible) {
      if (!selected.has(e.rel)) continue;
      if (e.left) toRight += 1;
      if (e.right) toLeft += 1;
    }
    return { toRight, toLeft };
  }, [visible, selected]);

  const toRightBlocked = pairState(left,"copyFrom",right,"copyTo").disabled || pairState(left,"compareDirs",right,"compareDirs").disabled;
  const toLeftBlocked = pairState(right,"copyFrom",left,"copyTo").disabled || pairState(right,"compareDirs",left,"compareDirs").disabled;
  const sameRoot = locationIdentity(parseLocation(left)) === locationIdentity(parseLocation(right));
  const copy = async (dir: "toRight" | "toLeft") => {
    if (loading || copyingRef.current || sameRoot) return;
    const copyRun=runToken.current;
    copyingRef.current=true; setCopying(true);
    try {
    const items: [string, string][] = [];
    for (const e of visible) {
      if (!selected.has(e.rel)) continue;
      if (dir === "toRight" && e.left) {
        items.push([joinPath(left, e.rel), joinPath(right, e.rel)]);
      } else if (dir === "toLeft" && e.right) {
        items.push([joinPath(right, e.rel), joinPath(left, e.rel)]);
      }
    }
    if (!items.length) return;

    // Synchronizing writes into the other side — asked for separately
    // from the read side, and re-asked here rather than trusting the
    // answer the comparison ran with (TICKET-031).
    const [from, to] = dir === "toRight" ? [left, right] : [right, left];
    const blocked = await explainPair(from, "copyFrom", to, "copyTo") ?? await explainPair(from, "compareDirs", to, "compareDirs");
    if (runToken.current !== copyRun) return;
    if (blocked) {
      setStatus({ ok: false, text: blocked });
      return;
    }

    const res = await syncCopy(items);
    if (runToken.current !== copyRun) return;

    // Refresh the affected windows and compare again.
    const s = usePanes.getState();
    (["left", "right"] as const).forEach((sd) => {
      const p = panelOf(s, sd).path;
      if (p === left || p === right) void s.refresh(sd);
    });
    const refreshedRun=runToken.current+1;
    await run(left, right, recursive);
    if (runToken.current!==refreshedRun) return;

    if (res.errors.length) {
      setStatus({ ok: false, text: t("op.errors") });
      useOps.getState().requestConfirm({
        title: t("op.errors"),
        message: describeItemErrors(res.errors).slice(0, 12).join("\n"),
        onConfirm: () => {},
      });
    } else {
      setStatus({
        ok: true,
        text: t("compare.copied").replace("{n}", String(res.ok)),
      });
    }
    } catch(e) { if (runToken.current===copyRun) setStatus({ok:false,text:describeErrorFull(e)}); }
    finally {copyingRef.current=false;setCopying(false);}
  };

  const FILTER_ROW: { key: Filter; label: TransKey; count: number }[] = [
    { key: "diff", label: "compare.filter.diff", count: counts.diff },
    { key: "leftOnly", label: "compare.filter.leftOnly", count: counts.leftOnly },
    { key: "rightOnly", label: "compare.filter.rightOnly", count: counts.rightOnly },
    { key: "same", label: "compare.filter.same", count: counts.same },
  ];

  const footer = (
    <div className="flex flex-wrap items-center gap-3 border-t border-edge px-4 py-2.5">
      <span className="text-[12px] text-dim">
        {t("compare.summary")
          .replace("{total}", String(counts.total))
          .replace("{visible}", String(visible.length))
          .replace("{selected}", String(selected.size))
          .replace("{same}", String(counts.same))
          .replace("{diff}", String(counts.diff))
          .replace("{left}", String(counts.leftOnly))
          .replace("{right}", String(counts.rightOnly))}
      </span>
      {entries.length>0 && (toLeftBlocked||toRightBlocked) && <span className="text-[12px] text-dim">{t("compare.copyUnavailable")}</span>}
      {sameRoot && <span className="text-[12px] text-dim">{t("compare.self")}</span>}
      {selected.size>0 && <span className="text-[12px] text-dim">{t("compare.unavailable").replace("{left}",String(selected.size-selCount.toLeft)).replace("{right}",String(selected.size-selCount.toRight))}</span>}
      {status && (
        <span
          className={cn("text-[12px]", status.ok ? "text-dim" : "text-red-400")}
        >
          {status.text}
        </span>
      )}
      <div className="ml-auto flex flex-wrap justify-end gap-2">
        <button
          onClick={() => void copy("toLeft")}
          disabled={selCount.toLeft === 0 || loading || copying || sameRoot || toLeftBlocked}
          className="flex items-center gap-1.5 rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent disabled:opacity-40"
        >
          <ChevronsLeft size={14} />
          {t("compare.toLeft")}
          {selCount.toLeft > 0 && ` (${selCount.toLeft})`}
        </button>
        <button
          onClick={() => void copy("toRight")}
          disabled={selCount.toRight === 0 || loading || copying || sameRoot || toRightBlocked}
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
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4">
            {/* Form: paths + refresh + filters */}
            <div className="flex flex-col gap-2">
              <div className="flex items-center gap-2 text-[12px]">
                <button
                  onClick={() => void run(left, right, recursive)}
                  title={t("compare.refresh")}
                  aria-label={t("compare.refresh")}
                  disabled={copying}
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

            <p className="text-[11px] text-dim">{t("compare.method")}</p>
            {error ? <div role="alert" className="text-red-400">{t("compare.error").replace("{err}",error)}</div> :
              <CompareResults left={left} right={right} entries={visible} selected={selected} toggle={toggleRow} toggleAll={toggleAll} run={runToken.current}/>}
            <div role="status" className="text-[12px] text-dim">{loading?t("compare.loading"):!entries.length?t("compare.noPairs"):!visible.length?t("compare.empty"):""}</div>
          </div>
    </AppDialog>
  );
}
