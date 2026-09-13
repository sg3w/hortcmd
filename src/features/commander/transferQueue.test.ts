// ============================================================
// Transfer queueing (TICKET-033).
//
// Copy and move run one at a time: a second request waits instead of
// competing for the same disk. Two things have to hold, and neither
// shows up as an error when it breaks — a queued transfer must keep the
// settings it was *started* with, and exactly one must be released when
// the running one ends, in the order they were requested.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";

interface StartCall {
  op: string;
  id: string;
  sources: string[];
  dest: string;
  limit: number;
  verify: boolean;
  bufKb: number;
  threads: number;
  safe: boolean;
}

const started: StartCall[] = [];

vi.mock("@/ipc/client", () => ({
  hasTauri: false,
  startTransfer: (
    op: string,
    id: string,
    sources: string[],
    dest: string,
    limit: number,
    verify: boolean,
    bufKb: number,
    threads: number,
    safe: boolean,
  ) => {
    started.push({ op, id, sources, dest, limit, verify, bufKb, threads, safe });
  },
  startExtract: () => {},
  startPack: () => {},
  deleteEntries: () => Promise.resolve({ ok: 0, errors: [] }),
  trashEntries: () => Promise.resolve({ ok: 0, errors: [] }),
  makeDir: () => Promise.resolve(""),
  renameEntry: () => Promise.resolve(),
  openPath: () => Promise.resolve(),
  openWith: () => Promise.resolve(),
  quickLook: () => Promise.resolve(),
  listDir: () => Promise.resolve({ path: "/", entries: [] }),
  listArchive: () => Promise.reject(new Error("none")),
  homeDir: () => Promise.resolve("/home"),
  requestGitStatus: () => {},
  locationCapabilities: () =>
    Promise.resolve({
      scheme: "local",
      store_id: "local",
      connected: true,
      caps: ["browse", "copyFrom", "copyTo", "move", "create", "deletePermanent"],
    }),
}));

const { useTransfers } = await import("@/store/transfersStore");
const { useSettings } = await import("@/store/settingsStore");
const { usePanes } = await import("@/store/panesStore");
const { loadCaps } = await import("@/store/capsStore");
const { runCopy: requestCopy, startNextQueued } = await import("./fileOps");
const { file } = await import("@/test/fixtures");

const { useOps } = await import("@/store/opsStore");
function runCopy(side: "left" | "right"): void {
  requestCopy(side);
  const confirm = useOps.getState().confirm;
  expect(confirm).not.toBeNull();
  useOps.getState().closeDialogs();
  confirm?.onConfirm();
}

/** Puts both panes on a folder with one selected file. */
function place(): void {
  const entry = file("a.txt");
  for (const side of ["left", "right"] as const) {
    usePanes.setState((s) => ({
      [side]: {
        ...s[side],
        tabs: [
          {
            ...s[side].tabs[0],
            path: side === "left" ? "/src" : "/dst",
            archive: null,
            raw: [entry],
            entries: [entry],
            cursor: 0,
            selected: new Set(["a.txt"]),
          },
        ],
        activeTab: 0,
      },
    }));
  }
}

beforeEach(async () => {
  started.length = 0;
  await loadCaps("/src");
  await loadCaps("/dst");
  place();
});

describe("one transfer at a time", () => {
  it("starts the first request immediately", () => {
    runCopy("left");
    expect(started).toHaveLength(1);
    expect(useTransfers.getState().transfers[0].queued).toBe(false);
  });

  it("queues a second request instead of starting it", () => {
    runCopy("left");
    runCopy("left");
    expect(started).toHaveLength(1);
    const [first, second] = useTransfers.getState().transfers;
    expect(first.queued).toBe(false);
    expect(second.queued).toBe(true);
  });

  it("releases exactly one, in request order, when the running one ends", () => {
    runCopy("left");
    runCopy("left");
    runCopy("left");
    const ids = useTransfers.getState().transfers.map((t) => t.id);

    // The running one finishes.
    useTransfers.getState().finish({
      id: ids[0],
      op: "copy",
      ok: 1,
      errors: [],
      cancelled: false,
      mode: "direct",
    });
    startNextQueued();

    expect(started.map((c) => c.id)).toEqual([ids[0], ids[1]]);
    expect(useTransfers.getState().transfers[2].queued).toBe(true);
  });

  it("releases nothing while one is still running", () => {
    runCopy("left");
    runCopy("left");
    startNextQueued();
    expect(started).toHaveLength(1);
  });

  it("drops a queued transfer without ever starting it", async () => {
    const { dequeueTransfer } = await import("./fileOps");
    runCopy("left");
    runCopy("left");
    const queuedId = useTransfers.getState().transfers[1].id;
    dequeueTransfer(queuedId);

    useTransfers.getState().finish({
      id: useTransfers.getState().transfers[0].id,
      op: "copy",
      ok: 1,
      errors: [],
      cancelled: false,
      mode: "direct",
    });
    startNextQueued();
    expect(started).toHaveLength(1);
  });
});

describe("settings are snapshotted at request time", () => {
  it("keeps the policy a queued transfer was requested with", () => {
    useSettings.setState({
      safeTransfers: true,
      verifyCopies: true,
      speedLimit: 100,
      copyThreads: 4,
    });
    runCopy("left"); // starts
    runCopy("left"); // queues, with these settings

    // The user changes their mind while it waits.
    useSettings.setState({
      safeTransfers: false,
      verifyCopies: false,
      speedLimit: 0,
      copyThreads: 1,
    });

    useTransfers.getState().finish({
      id: useTransfers.getState().transfers[0].id,
      op: "copy",
      ok: 1,
      errors: [],
      cancelled: false,
      mode: "safe",
    });
    startNextQueued();

    const released = started[1];
    expect(released.safe).toBe(true);
    expect(released.verify).toBe(true);
    expect(released.limit).toBe(100 * 1024);
    expect(released.threads).toBe(4);
  });
});
