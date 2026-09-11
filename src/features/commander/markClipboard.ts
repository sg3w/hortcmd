// ============================================================
// The four clipboard variants of the "Mark" menu (TICKET-059).
//
// All four share one target set — the marked *visible* entries, or the
// cursor entry when nothing is marked, never "..". They also share the
// existing export formatter (`exportList.ts`), so a detail column in
// the clipboard is formatted exactly like the same column in an
// exported file list and the two can never drift apart.
//
// Size and date follow the current display settings. Metadata a remote
// provider did not deliver stays empty rather than being invented as
// "0 B" or as a permission string nobody checked.
// ============================================================

import { panelOf, usePanes, type Row, type Side } from "@/store/panesStore";
import { useSettings } from "@/store/settingsStore";
import { notify } from "@/store/noticeStore";
import { writeClipboard } from "@/lib/clipboard";
import { joinPath } from "@/lib/path";
import { buildExport, type ExportField } from "./exportList";

/** What a mark command acts on: marked visible rows, else the cursor. */
export function targetRows(side: Side): Row[] {
  const p = panelOf(usePanes.getState(), side);
  const rows = p.entries.filter((r) => !r.parent);
  const marked = rows.filter((r) => p.selected.has(r.name));
  if (marked.length > 0) return marked;
  const cur = p.entries[p.cursor];
  return cur && !cur.parent ? [cur] : [];
}

/** Writes to the clipboard and reports a refused write in the pane. */
async function put(side: Side, text: string): Promise<void> {
  try {
    await writeClipboard(text);
  } catch (e) {
    notify(side, "sel.copyError", { err: String(e) }, "error");
  }
}

/** One plain name per line. */
export function copyNames(side: Side): void {
  void put(side, targetRows(side).map((r) => r.name).join("\n"));
}

/**
 * One full address per line: a local path, or the provider address of a
 * remote entry — which carries no password or token, because the pane
 * path never holds one.
 */
export function copyPaths(side: Side): void {
  const p = panelOf(usePanes.getState(), side);
  void put(
    side,
    targetRows(side)
      .map((r) => joinPath(p.path, r.name))
      .join("\n"),
  );
}

/** Detail columns; "path" replaces "name" in the path variant. */
const DETAIL_FIELDS: ExportField[] = ["ext", "size", "modified", "type", "perms"];

/** TSV with a header row; tabs, quotes and line breaks are quoted. */
function copyDetails(side: Side, withPath: boolean): void {
  const p = panelOf(usePanes.getState(), side);
  const { sizeFormat, dateFormat } = useSettings.getState();
  const text = buildExport(
    targetRows(side),
    p.path,
    {
      format: "tsv",
      fields: [withPath ? "path" : "name", ...DETAIL_FIELDS],
      // The rows are already picked; these two only matter to `pickRows`.
      includeFolders: true,
      onlySelected: false,
      header: true,
      formatValues: true,
    },
    { sizeFormat, dateFormat },
  );
  void put(side, text);
}

/** Name, extension, size, date, type and permissions. */
export function copyNameDetails(side: Side): void {
  copyDetails(side, false);
}

/** The same columns, but with the full path instead of the name. */
export function copyPathDetails(side: Side): void {
  copyDetails(side, true);
}
