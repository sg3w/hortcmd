// ============================================================
// The pure pane transitions (TICKET-035).
//
// These run without a store, without IPC and without React, which is
// the point of extracting them: the places where a mistake is *silent*
// — a selection that still names deleted files, a cursor that lands on
// the wrong row after a sort — can now be stated as plain input and
// output.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import { dir, file } from "@/test/fixtures";
import type { Row, TabState } from "./state";
import {
  applyShiftSelection,
  applySort,
  clamp,
  cursorFor,
  locationOf,
  newTab,
  pruneSelection,
  rebuildEntries,
  sortEntries,
} from "./state";

vi.mock("@/ipc/client", () => ({ hasTauri: false }));

const { useSettings } = await import("../settingsStore");

beforeEach(() => {
  useSettings.setState({ hideSystemFiles: false, foldersFirst: false });
});

const rows = (...names: string[]): Row[] => names.map((n) => file(n));

describe("sorting", () => {
  it("sorts by name, case- and accent-insensitively", () => {
    const list = rows("Beta.txt", "alpha.txt", "Ähnlich.txt");
    sortEntries(list, "name", true);
    expect(list.map((r) => r.name)).toEqual([
      "Ähnlich.txt",
      "alpha.txt",
      "Beta.txt",
    ]);
  });

  it("sorts numerically inside a name", () => {
    // "file10" after "file9" — a plain string sort gets this wrong.
    const list = rows("file10.txt", "file9.txt", "file1.txt");
    sortEntries(list, "name", true);
    expect(list.map((r) => r.name)).toEqual([
      "file1.txt",
      "file9.txt",
      "file10.txt",
    ]);
  });

  it("reverses on descending", () => {
    const list = rows("a", "b", "c");
    sortEntries(list, "name", false);
    expect(list.map((r) => r.name)).toEqual(["c", "b", "a"]);
  });

  it("sorts by size and by date", () => {
    const list = [
      file("a", { size: 30, modified: 1 }),
      file("b", { size: 10, modified: 3 }),
      file("c", { size: 20, modified: 2 }),
    ];
    sortEntries(list, "size", true);
    expect(list.map((r) => r.name)).toEqual(["b", "c", "a"]);
    sortEntries(list, "date", true);
    expect(list.map((r) => r.name)).toEqual(["a", "c", "b"]);
  });

  it("puts folders first when that setting is on", () => {
    useSettings.setState({ foldersFirst: true });
    const list = [file("a.txt"), dir("zzz"), file("b.txt")];
    sortEntries(list, "name", true);
    expect(list[0].name).toBe("zzz");
  });

  it("keeps a stable order for equal keys, by name", () => {
    const list = [
      file("b", { size: 5 }),
      file("a", { size: 5 }),
      file("c", { size: 5 }),
    ];
    sortEntries(list, "size", true);
    expect(list.map((r) => r.name)).toEqual(["a", "b", "c"]);
  });
});

describe("rebuilding the displayed rows", () => {
  const raw = [file("a.txt"), file(".hidden", { hidden: true }), dir("sub")];

  it('adds a ".." row below the root and not at it', () => {
    expect(rebuildEntries(raw, "/Users/x", { key: "name", asc: true }, false)[0])
      .toMatchObject({ name: "..", parent: true });
    expect(
      rebuildEntries(raw, "/", { key: "name", asc: true }, false).some(
        (r) => r.parent,
      ),
    ).toBe(false);
  });

  it('always adds ".." inside an archive, so it can be left', () => {
    expect(
      rebuildEntries(raw, "/", { key: "name", asc: true }, true)[0],
    ).toMatchObject({ parent: true });
  });

  it("drops hidden entries only when the setting says so", () => {
    const names = () =>
      rebuildEntries(raw, "/", { key: "name", asc: true }, false).map(
        (r) => r.name,
      );
    expect(names()).toContain(".hidden");
    useSettings.setState({ hideSystemFiles: true });
    expect(names()).not.toContain(".hidden");
  });

  it("applies the quick filter to names, case-insensitively", () => {
    const names = rebuildEntries(
      raw,
      "/",
      { key: "name", asc: true },
      false,
      "A.TX",
    ).map((r) => r.name);
    expect(names).toEqual(["a.txt"]);
  });

  it("hides equal files but never folders or the parent row", () => {
    // The comparison filter must not make a folder unreachable.
    const hidden = new Set(["a.txt", "sub"]);
    const names = rebuildEntries(
      raw,
      "/Users/x",
      { key: "name", asc: true },
      false,
      "",
      hidden,
    ).map((r) => r.name);
    expect(names).toContain("..");
    expect(names).toContain("sub");
    expect(names).not.toContain("a.txt");
  });

  it("copies the raw entries rather than aliasing them", () => {
    const built = rebuildEntries(raw, "/", { key: "name", asc: true }, false);
    expect(built.find((r) => r.name === "a.txt")).not.toBe(raw[0]);
  });
});

describe("selection pruning", () => {
  it("keeps only names that still exist", () => {
    const kept = pruneSelection(
      new Set(["a.txt", "gone.txt"]),
      rows("a.txt", "b.txt"),
    );
    expect([...kept]).toEqual(["a.txt"]);
  });

  it("returns an empty set when nothing survives", () => {
    expect(pruneSelection(new Set(["gone"]), rows("a")).size).toBe(0);
  });
});

describe("cursor placement", () => {
  const list = rows("a", "b", "c");

  it("follows the entry it was on", () => {
    expect(cursorFor(list, "c", 0)).toBe(2);
  });

  it("falls back to the old index when the entry is gone", () => {
    expect(cursorFor(list, "gone", 1)).toBe(1);
  });

  it("never points past the end", () => {
    expect(cursorFor(list, undefined, 99)).toBeLessThan(list.length);
    expect(cursorFor([], undefined, 5)).toBe(0);
  });

  it("clamps into range", () => {
    expect(clamp(-3, 0, 10)).toBe(0);
    expect(clamp(99, 0, 10)).toBe(10);
    expect(clamp(4, 0, 10)).toBe(4);
  });
});

describe("sort transition on a tab", () => {
  function tab(): TabState {
    const t = newTab("/Users/x");
    t.raw = [file("b.txt"), file("a.txt")];
    t.entries = rebuildEntries(t.raw, t.path, t.sort, false);
    t.cursor = 1;
    t.shiftBase = new Set(["a.txt"]);
    return t;
  }

  it("re-sorts the rows and resets cursor and anchor", () => {
    const next = applySort(tab(), { key: "name", asc: false });
    expect(next.sort).toEqual({ key: "name", asc: false });
    expect(next.cursor).toBe(0);
    expect(next.anchor).toBe(0);
    // A range selection from before the sort would now cover the wrong
    // rows, so it is dropped rather than carried over.
    expect(next.shiftBase).toBeNull();
  });

  it("leaves the original tab untouched", () => {
    const before = tab();
    const snapshot = before.sort;
    applySort(before, { key: "size", asc: true });
    expect(before.sort).toBe(snapshot);
  });
});

describe("range selection", () => {
  function tab(cursor: number, anchor: number, base?: Set<string>): TabState {
    const t = newTab("/Users/x");
    t.raw = [file("a"), file("b"), file("c"), file("d")];
    t.entries = t.raw.map((e) => ({ ...e }));
    t.cursor = cursor;
    t.anchor = anchor;
    t.shiftBase = base ?? null;
    return t;
  }

  it("selects from the anchor to the cursor, inclusive", () => {
    const next = applyShiftSelection(tab(0, 0), 2);
    expect([...next.selected].sort()).toEqual(["a", "b", "c"]);
  });

  it("works upwards as well", () => {
    const next = applyShiftSelection(tab(3, 3), 1);
    expect([...next.selected].sort()).toEqual(["b", "c", "d"]);
  });

  it("shrinks again when the direction reverses", () => {
    // The snapshot is what makes a shrink possible: without it the
    // selection could only ever grow.
    const grown = applyShiftSelection(tab(0, 0), 3);
    const shrunk = applyShiftSelection({ ...grown, cursor: 3 }, 1);
    expect([...shrunk.selected].sort()).toEqual(["a", "b"]);
  });

  it("keeps what was selected before the range started", () => {
    const t = tab(1, 1);
    t.selected = new Set(["d"]);
    const next = applyShiftSelection(t, 2);
    expect([...next.selected].sort()).toEqual(["b", "c", "d"]);
  });
});

describe("the canonical location of a tab", () => {
  it("is the folder for a plain pane", () => {
    expect(locationOf({ path: "/Users/x", archive: null })).toBe("/Users/x");
  });

  it("names the archive and the inner path inside one", () => {
    const loc = locationOf({ path: "/inner", archive: "/Users/x/a.zip" });
    expect(loc).toContain("/Users/x/a.zip");
    expect(loc).toContain("/inner");
  });
});
