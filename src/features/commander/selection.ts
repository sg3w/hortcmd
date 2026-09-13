// ============================================================
// The selection commands behind the "Mark" menu (TICKET-059).
//
// Every one of them works on the active tab of one pane and on the
// rows that are actually *visible* there: an entry hidden by the quick
// filter or by the comparison filter may never end up marked, because
// a mark is what a later copy or delete acts on.
//
// General actions include folders (TICKET-060); only extension and
// content comparisons retain file-specific semantics.
// ============================================================

import { locationOf, panelOf, usePanes, type Side } from "@/store/panesStore";
import { useOps } from "@/store/opsStore";
import { notify } from "@/store/noticeStore";
import { translate } from "@/store/settingsStore";
import { splitName } from "@/lib/format";

/** "*": invert the visible file and folder selection. */
export function invertSelection(side: Side): void {
  usePanes.getState().invertSelection(side);
}

/** Mark every visible file and folder. */
export function selectAllFiles(side: Side): void {
  usePanes.getState().selectAllFiles(side);
}

/** Clear the whole selection of the active tab, folders included. */
export function selectNoFiles(side: Side): void {
  usePanes.getState().clearSelection(side);
}

/** Mark every visible file with the cursor file's extension. */
export function selectSameExt(side: Side): void {
  usePanes.getState().selectSameExt(side);
}

/** "+"/"-": ask for a pattern and select/deselect the matches. */
export function promptSelect(side: Side, add: boolean): void {
  const p = panelOf(usePanes.getState(), side);
  const cur = p.entries[p.cursor];
  const ext = cur && !cur.parent && !cur.is_dir ? splitName(cur).ext : "";
  const initial = ext ? `*.${ext}` : "*";

  useOps.getState().requestPrompt({
    title: translate(add ? "sel.select.title" : "sel.unselect.title"),
    label: translate("sel.pattern"),
    initial,
    confirmLabel: translate("op.confirm"),
    onSubmit: (pattern) =>
      usePanes
        .getState()
        .selectPattern(side, pattern.trim() || "*", add),
  });
}

/** Store the current selection as this tab's snapshot. */
export function rememberSelection(side: Side): void {
  usePanes.getState().rememberSelection(side);
  const size = panelOf(usePanes.getState(), side).selected.size;
  notify(side, "sel.remembered", { n: size });
}

/** True when this tab holds a snapshot for the folder it is showing. */
export function hasSnapshot(side: Side): boolean {
  const t = panelOf(usePanes.getState(), side);
  return t.snapshot !== null && t.snapshot.location === locationOf(t);
}

/** Restore the snapshot and report what is gone. */
export function restoreSelection(side: Side): void {
  if (!hasSnapshot(side)) return;
  const { applied, missing } = usePanes.getState().restoreSelection(side);
  notify(
    side,
    missing > 0 ? "sel.restored.partial" : "sel.restored",
    { n: applied, missing },
    missing > 0 ? "error" : "info",
  );
}
