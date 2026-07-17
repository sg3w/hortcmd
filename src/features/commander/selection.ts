// ============================================================
// Multi-selection patterns: "+" select, "-" deselect
// (each via wildcard), "*" invert the selection.
// ============================================================

import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useOps } from "@/store/opsStore";
import { translate } from "@/store/settingsStore";
import { splitName } from "@/lib/format";

/** "*": invert the selection (toggle all non-".." entries). */
export function invertSelection(side: Side): void {
  usePanes.getState().invertSelection(side);
}

/** "+"/"-": prompt for a pattern and select/deselect matching entries. */
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
      usePanes.getState().selectPattern(side, pattern.trim() || "*", add),
  });
}
