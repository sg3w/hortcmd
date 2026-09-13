// ============================================================
// Bar below the list carrying two temporary states of a pane
// (TICKET-059):
//
//   · the comparison filter — visible whenever equal files are hidden,
//     with the "clear" action right where the effect is, so the hidden
//     files are never a silent state;
//   · a transient result message (selection restored, comparison done,
//     clipboard refused).
//
// Both are strictly about the last action, not about the folder, and
// both disappear as soon as the pane moves on.
// ============================================================

import { useEffect } from "react";
import { GitCompareArrows, Info, X } from "lucide-react";
import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { fillVars, useNotices } from "@/store/noticeStore";
import { useT } from "@/store/settingsStore";
import { describeErrorFull } from "@/store/errorText";
import { cn } from "@/lib/cn";

/** How long a result message stays before it fades out by itself. */
const NOTICE_MS = 8000;

export function PaneNotice({ side }: { side: Side }) {
  const freshness = usePanes(s => panelOf(s, side).freshness);
  const loadError = usePanes(s => panelOf(s, side).loadError);
  const refreshError = usePanes(s => panelOf(s, side).refreshError);
  const hidden = usePanes((s) => panelOf(s, side).compareHidden);
  const clearCompare = usePanes((s) => s.clearCompareFilter);
  const notice = useNotices((s) => s[side]);
  const clearNotice = useNotices((s) => s.clear);
  const t = useT();

  // Bound to `seq`, so re-running the same command restarts the timer
  // instead of letting the first one cut the second message short.
  useEffect(() => {
    if (!notice) return;
    const id = window.setTimeout(() => clearNotice(side), NOTICE_MS);
    return () => window.clearTimeout(id);
  }, [notice?.seq, notice, side, clearNotice]);

  const stale = freshness !== "current" || loadError || refreshError;
  if (!hidden && !notice && !stale) return null;

  return (
    <div className="flex-shrink-0 border-t border-edge">
      {stale && <div role="status" className="flex items-center gap-2 bg-header px-2 py-1 text-[12px]">
        <span className="min-w-0 flex-1 text-dim">
          {t(freshness === "offline" ? "refresh.offline" : freshness === "manual" ? "refresh.manual" : "refresh.stale")}
          {(loadError || refreshError) ? ` ${describeErrorFull(loadError || refreshError)}` : ""}
        </span>
        <button className="shrink-0 text-accent" onClick={() => void usePanes.getState().refresh(side)}>{t("tb.refresh")}</button>
      </div>}
      {hidden && (
        <div className="flex items-center gap-2 bg-accent-dim px-2 py-1 text-[12px]">
          <GitCompareArrows size={13} className="shrink-0 text-accent" />
          <span className="shrink-0 text-dim">{t("cmp.bar.label")}</span>
          <span className="min-w-0 flex-1 truncate text-text">
            {fillVars(t("cmp.bar.hidden"), { n: hidden.size })}
          </span>
          <button
            onClick={() => clearCompare()}
            title={t("cmp.bar.clear")}
            className="shrink-0 rounded p-0.5 text-dim hover:bg-header hover:text-text"
          >
            <X size={13} />
          </button>
        </div>
      )}
      {notice && (
        <div className="flex items-center gap-2 bg-header px-2 py-1 text-[12px]">
          <Info
            size={13}
            className={cn(
              "shrink-0",
              notice.tone === "error" ? "text-red-400" : "text-accent",
            )}
          />
          <span
            className={cn(
              "min-w-0 flex-1 truncate",
              notice.tone === "error" ? "text-red-400" : "text-text",
            )}
          >
            {fillVars(t(notice.key), notice.vars)}
          </span>
          <button
            onClick={() => clearNotice(side)}
            title={t("filter.clear")}
            className="shrink-0 rounded p-0.5 text-dim hover:bg-panel hover:text-text"
          >
            <X size={13} />
          </button>
        </div>
      )}
    </div>
  );
}
