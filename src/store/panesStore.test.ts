// ============================================================
// Pane behaviours that touch data safety (TICKET-033).
//
// These are the paths where a mistake silently acts on the wrong
// entries: a refresh that keeps a selection pointing at files that are
// gone, a reveal that puts the cursor on the wrong row after a copy,
// and a session restore that would otherwise start with no folder at
// all. None of them raise an error when they go wrong — they just do
// the wrong thing quietly, which is why they are pinned here.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DirListing } from "@/ipc/bindings";
import { file as entry } from "@/test/fixtures";

/** Folder contents the fake backend serves, by path. */
let folders: Record<string, string[]> = {};
/** Paths whose listing must fail (folder gone). */
let missing = new Set<string>();

vi.mock("@/ipc/client", () => ({
  hasTauri: () => false,
  listDir: (path: string): Promise<DirListing> => {
    if (missing.has(path)) return Promise.reject(new Error("weg"));
    const names = folders[path];
    if (!names) return Promise.reject(new Error(`unknown folder ${path}`));
    return Promise.resolve({ path, entries: names.map((n) => entry(n)) });
  },
  listArchive: () => Promise.reject(new Error("no archives here")),
  homeDir: () => Promise.resolve("/home"),
  requestGitStatus: () => {},
  // The pane primes capabilities on navigation and asks before
  // requesting a Git status; both go through this one command.
  locationCapabilities: () =>
    Promise.resolve({
      scheme: "local",
      store_id: "local",
      connected: true,
      caps: ["browse", "gitStatus"],
    }),
}));

const { panelOf, usePanes } = await import("./panesStore");

/** The active tab of a side. */
const tab = (side: "left" | "right" = "left") =>
  panelOf(usePanes.getState(), side);

beforeEach(() => {
  folders = {
    "/dir": ["a.txt", "b.txt", "c.txt"],
    "/home": ["home.txt"],
    "/other": ["x.txt"],
  };
  missing = new Set();
});

describe("selection across a refresh", () => {
  it("keeps the entries that are still there and drops the rest", async () => {
    await usePanes.getState().loadDir("left", "/dir");
    usePanes.setState((s) => ({
      left: {
        ...s.left,
        tabs: [{ ...s.left.tabs[0], selected: new Set(["a.txt", "c.txt"]) }],
      },
    }));

    folders["/dir"] = ["a.txt", "b.txt"]; // c.txt was deleted elsewhere
    await usePanes.getState().refresh("left");

    // A selection naming a file that no longer exists would make the
    // next delete or copy act on a stale set.
    expect([...tab().selected]).toEqual(["a.txt"]);
  });

  it("keeps the cursor inside the list when the folder shrinks", async () => {
    await usePanes.getState().loadDir("left", "/dir");
    usePanes.setState((s) => ({
      left: { ...s.left, tabs: [{ ...s.left.tabs[0], cursor: 3 }] },
    }));

    folders["/dir"] = ["a.txt"];
    await usePanes.getState().refresh("left");

    expect(tab().cursor).toBeLessThan(tab().entries.length);
  });
});

describe("reveal after a refresh", () => {
  it("selects the named entries and puts the cursor on the first", async () => {
    await usePanes.getState().loadDir("left", "/dir");
    folders["/dir"] = ["a.txt", "b.txt", "c.txt", "neu.txt"];

    await usePanes.getState().refresh("left", {
      reveal: true,
      names: ["neu.txt"],
    });

    expect([...tab().selected]).toEqual(["neu.txt"]);
    expect(tab().entries[tab().cursor].name).toBe("neu.txt");
  });

  it("reveals by name, not by diffing against the previous listing", async () => {
    // A watcher refresh may already have brought the new entry in, so a
    // diff would find "nothing new" and leave the cursor behind.
    await usePanes.getState().loadDir("left", "/dir");
    folders["/dir"] = ["a.txt", "b.txt", "c.txt", "neu.txt"];
    await usePanes.getState().refresh("left"); // watcher got there first

    await usePanes.getState().refresh("left", {
      reveal: true,
      names: ["neu.txt"],
    });
    expect(tab().entries[tab().cursor].name).toBe("neu.txt");
  });

  it("leaves cursor and selection alone when nothing was revealed", async () => {
    await usePanes.getState().loadDir("left", "/dir");
    usePanes.setState((s) => ({
      left: {
        ...s.left,
        tabs: [{ ...s.left.tabs[0], cursor: 2, selected: new Set(["b.txt"]) }],
      },
    }));

    await usePanes.getState().refresh("left", { reveal: true, names: ["gone"] });

    expect(tab().entries[tab().cursor].name).toBe("b.txt");
    expect([...tab().selected]).toEqual(["b.txt"]);
  });
});

describe("session restore", () => {
  const saved = (paths: string[]) => ({
    tabs: paths.map((path) => ({ path })),
    activeTab: 0,
    viewMode: "details" as const,
  });

  it("restores the saved folders", async () => {
    await usePanes.getState().restoreSession({
      left: saved(["/dir"]),
      right: saved(["/other"]),
      active: "left",
    });
    expect(tab("left").path).toBe("/dir");
    expect(tab("right").path).toBe("/other");
  });

  it("skips a folder that is gone and keeps the rest", async () => {
    missing.add("/other");
    await usePanes.getState().restoreSession({
      left: saved(["/other", "/dir"]),
      right: saved(["/dir"]),
      active: "left",
    });
    expect(usePanes.getState().left.tabs).toHaveLength(1);
    expect(tab("left").path).toBe("/dir");
  });

  it("falls back to home when no saved folder survives", async () => {
    missing.add("/dir").add("/other");
    await usePanes.getState().restoreSession({
      left: saved(["/dir", "/other"]),
      right: saved(["/dir"]),
      active: "left",
    });
    expect(tab("left").path).toBe("/home");
    expect(usePanes.getState().left.tabs).toHaveLength(1);
  });

  it("clamps a saved active-tab index that no longer exists", async () => {
    missing.add("/other");
    await usePanes.getState().restoreSession({
      left: { ...saved(["/dir", "/other"]), activeTab: 1 },
      right: saved(["/dir"]),
      active: "left",
    });
    expect(usePanes.getState().left.activeTab).toBe(0);
  });
});
