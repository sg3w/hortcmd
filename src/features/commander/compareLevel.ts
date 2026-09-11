// ============================================================
// The two pane comparisons of the "Mark" menu (TICKET-059).
//
// Both compare only the *visible directory level* of the two active
// tabs and both only ever change marks and visibility — nothing is
// copied, deleted or synchronised. The full recursive synchronisation
// dialog under "Commands" is untouched and stays the place for that.
//
// The comparison runs on the listings the panes already hold, so it
// serves local folders, every remote provider and the inside of an
// archive from one code path and without a second directory scan. The
// classification itself lives in Rust (`compare_levels`) next to the
// recursive comparison, so both share one definition of "equal".
//
// Equality here is metadata equality — name, size and timestamp. That
// is explicitly not a guarantee about content; entries whose metadata
// cannot carry that verdict (symlinks, missing timestamps, a name that
// is a folder on one side) come back as "unknown" and stay visible.
// ============================================================

import type { LevelDiff, LevelEntry } from "@/ipc/bindings";
import { compareLevels } from "@/ipc/client";
import { connectionFor } from "@/features/net/netStore";
import {
  locationOf,
  panelOf,
  usePanes,
  type CompareApply,
  type Side,
} from "@/store/panesStore";
import { notify } from "@/store/noticeStore";
import { schemeOf } from "@/lib/path";

/** Runs while a comparison is in flight (the menu entries disable). */
let running = false;
/** Identifies the newest request; older answers are dropped. */
let generation = 0;

export function compareBusy(): boolean {
  return running;
}

/** True when both panes currently sit on a reachable folder. */
export function compareReady(): boolean {
  const s = usePanes.getState();
  return (["left", "right"] as const).every((side) => {
    const p = panelOf(s, side);
    if (!p.path) return false;
    // A remote folder whose connection is gone cannot be compared, and
    // neither can a listing we never received.
    return schemeOf(p.path) === null || connectionFor(p.path) !== null;
  });
}

/** True when at least one pane currently hides equal files. */
export function compareFilterActive(): boolean {
  const s = usePanes.getState();
  return (["left", "right"] as const).some(
    (side) => panelOf(s, side).compareHidden !== null,
  );
}

/** The raw listing of one side, reduced to what the comparison needs. */
function levelOf(side: Side): LevelEntry[] {
  const p = panelOf(usePanes.getState(), side);
  return p.raw.map((e) => ({
    name: e.name,
    is_dir: e.is_dir,
    is_symlink: e.is_symlink,
    size: e.size,
    modified: e.modified,
  }));
}

/** Identity of both tabs, so a late answer for other folders is dropped. */
function stamp(): string {
  const s = usePanes.getState();
  return `${locationOf(panelOf(s, "left"))} ${locationOf(panelOf(s, "right"))}`;
}

/** Visible file names of one side (folders and ".." never take part). */
function visibleFiles(side: Side): Set<string> {
  const p = panelOf(usePanes.getState(), side);
  return new Set(
    p.entries.filter((r) => !r.parent && !r.is_dir).map((r) => r.name),
  );
}

type Mode = "differences" | "newerHideSame";

/**
 * Compares both panes and applies the result.
 *
 * `differences` replaces the file selection on both sides with the
 * files that are missing on the other side or differ from it.
 * `newerHideSame` marks only what is missing or newer per side and
 * hides the files that are equal on both sides.
 */
export async function runCompare(mode: Mode): Promise<void> {
  if (running || !compareReady()) return;
  // A new comparison always starts from an unfiltered view, so the
  // previous result can never narrow the new one.
  usePanes.getState().clearCompareFilter();

  const before = stamp();
  const mine = ++generation;
  running = true;
  let diffs: LevelDiff[];
  try {
    diffs = await compareLevels(levelOf("left"), levelOf("right"));
  } catch (e) {
    notify(usePanes.getState().active, "cmp.error", { err: String(e) }, "error");
    return;
  } finally {
    running = false;
  }
  // Dropped when a newer comparison started or when either pane moved
  // on in the meantime — a stale result must never mark anything.
  if (mine !== generation || stamp() !== before) return;

  const visible = { left: visibleFiles("left"), right: visibleFiles("right") };
  const select: Record<Side, Set<string>> = { left: new Set(), right: new Set() };
  const hidden: Record<Side, Set<string>> = { left: new Set(), right: new Set() };

  for (const d of diffs) {
    const onSide = (side: Side) =>
      (side === "left" ? d.left : d.right) && visible[side].has(d.name);
    const mark = (side: Side) => {
      if (onSide(side)) select[side].add(d.name);
    };
    switch (d.status) {
      case "same":
        if (mode === "newerHideSame") {
          // Only equal files disappear, and only on both sides at once.
          if (onSide("left") && onSide("right")) {
            hidden.left.add(d.name);
            hidden.right.add(d.name);
          }
        }
        break;
      case "left_only":
        mark("left");
        break;
      case "right_only":
        mark("right");
        break;
      case "newer_left":
        mark("left");
        if (mode === "differences") mark("right");
        break;
      case "newer_right":
        mark("right");
        if (mode === "differences") mark("left");
        break;
      default:
        // "different" and "unknown": a real difference, but with no
        // newer side. Marked when asked for differences, otherwise left
        // visible and unmarked — never hidden as equal.
        if (mode === "differences") {
          mark("left");
          mark("right");
        }
    }
  }

  const apply: Record<Side, CompareApply> = {
    left: {
      hidden: mode === "newerHideSame" ? hidden.left : null,
      selected: select.left,
    },
    right: {
      hidden: mode === "newerHideSame" ? hidden.right : null,
      selected: select.right,
    },
  };
  usePanes.getState().applyCompare(apply);

  const marked = select.left.size + select.right.size;
  const side = usePanes.getState().active;
  if (marked === 0 && hidden.left.size === 0) {
    notify(side, "cmp.none");
  } else {
    notify(side, "cmp.marked", { n: marked });
  }
}

/** Menu/indicator action: bring the hidden equal files back. */
export function clearCompareFilter(): void {
  usePanes.getState().clearCompareFilter();
}
