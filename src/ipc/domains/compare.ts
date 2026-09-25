// ============================================================
// Comparison and synchronization (TICKET-035).
//
// Two directory trees, two single files, and the one-level pane
// comparison (ticket 059), plus the selection files that persist a
// marked set.
// ============================================================

import type {
  DiffEntry,
  FileDiff,
  LevelDiff,
  LevelEntry,
  OpResult,
  SelEntry,
  SelectionFile,
} from "../bindings";
import { hasTauri, invoke } from "../invoke";
import * as demo from "../demo";

/**
 * Compares two directory trees (by size + modification time) and delivers
 * the results **streaming per directory** over `onBatch`, so large
 * trees don't appear only at the end. Resolves with `true` if the result
 * was truncated at the upper limit (200,000 files).
 */
export async function compareDirs(
  left: string,
  right: string,
  recursive: boolean,
  onBatch: (entries: DiffEntry[]) => void,
): Promise<boolean> {
  if (!hasTauri()) {
    const rows = demo.demoCompare(recursive);
    const top = rows.filter((r) => !r.rel.includes("/"));
    const nested = rows.filter((r) => r.rel.includes("/"));
    onBatch(top);
    if (nested.length) {
      await new Promise((res) => setTimeout(res, 250));
      onBatch(nested);
    }
    return false;
  }
  const { Channel } = await import("@tauri-apps/api/core");
  const channel = new Channel<DiffEntry[]>();
  channel.onmessage = onBatch;
  return invoke<boolean>("compare_dirs", {
    left,
    right,
    recursive,
    onBatch: channel,
  });
}

/**
 * Compares the two directory levels the panes already hold (TICKET-059).
 * No directory is scanned again, so the same call serves local folders,
 * every remote provider and the inside of an archive alike.
 */
export async function compareLevels(
  left: LevelEntry[],
  right: LevelEntry[],
): Promise<LevelDiff[]> {
  if (!hasTauri()) return demo.demoCompareLevels(left, right);
  return invoke<LevelDiff[]>("compare_levels", { left, right });
}

/**
 * Writes a selection file. Like `write_text_file`, `false` reports an
 * existing target instead of overwriting it.
 */
export async function saveSelection(
  path: string,
  location: string,
  entries: SelEntry[],
  overwrite: boolean,
): Promise<boolean> {
  if (!hasTauri()) {
    console.log("[demo] save_selection", path, `${entries.length} Eintrag/Einträge`);
    return true;
  }
  return invoke<boolean>("save_selection", { path, location, entries, overwrite });
}

/** Reads and validates a selection file (format checks live in Rust). */
export async function loadSelection(path: string): Promise<SelectionFile> {
  if (!hasTauri()) {
    return { version: 1, location: "/demo", entries: [{ name: "readme.md", is_dir: false }] };
  }
  return invoke<SelectionFile>("load_selection", { path });
}

/** Copies files source→target (pairs of absolute paths). */
export async function syncCopy(
  items: [string, string][],
): Promise<OpResult> {
  if (!hasTauri()) {
    console.log("[demo] sync_copy", `${items.length} Datei(en)`);
    return { ok: items.length, errors: [] };
  }
  return invoke<OpResult>("sync_copy", { items });
}

/** Compares two files by content (text diff or hex comparison). */
export async function compareFiles(
  left: string,
  right: string,
): Promise<FileDiff> {
  if (!hasTauri()) return demo.demoFileDiff(left, right);
  return invoke<FileDiff>("compare_files", { left, right });
}
