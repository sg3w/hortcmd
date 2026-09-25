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
  checkSpace?: boolean;
}

const started: StartCall[] = [];
const extracted: string[] = [];

vi.mock("@/ipc/client", () => ({
  hasTauri: () => false,
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
    checkSpace?: boolean,
  ) => {
    started.push({ op, id, sources, dest, limit, verify, bufKb, threads, safe, checkSpace });
  },
  startExtract: (id: string) => { extracted.push(id); },
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
      caps: ["browse", "copyFrom", "copyTo", "move", "create", "deletePermanent", "archiveExtractFrom", "archiveExtractTo"],
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
  extracted.length = 0;
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
      checkTransferSpace: false,
      verifyCopies: true,
      speedLimit: 100,
      copyThreads: 4,
    });
    runCopy("left"); // starts
    runCopy("left"); // queues, with these settings

    // The user changes their mind while it waits.
    useSettings.setState({
      safeTransfers: false,
      checkTransferSpace: true,
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
    expect(released.checkSpace).toBe(false);
    expect(released.verify).toBe(true);
    expect(released.limit).toBe(100 * 1024);
    expect(released.threads).toBe(4);
  });
});

describe("extraction serialization", () => {
  function extract() {
    usePanes.setState(s => ({ left: { ...s.left, tabs: [{ ...s.left.tabs[0], archive: "/source.zip", path: "/" }] } }));
    requestCopy("left");
  }
  function done(id: string) {
    useTransfers.getState().finish({ id, op: "extract", ok: 1, errors: [], cancelled: false, mode: "safe" });
    startNextQueued();
  }
  it.each([true, false])("serializes repeated extraction with queueTransfers=%s", queueTransfers => {
    useSettings.setState({ queueTransfers });
    extract(); extract(); extract();
    const [first, second, third] = useTransfers.getState().transfers;
    expect(extracted).toEqual([first.id]);
    expect(second.queued).toBe(true);
    expect(third.queued).toBe(true);
    startNextQueued();
    expect(extracted).toEqual([first.id]);
    done(first.id);
    expect(extracted).toEqual([first.id, second.id]);
    done(second.id);
    expect(extracted).toEqual([first.id, second.id, third.id]);
    done(third.id);
  });
  it("cancels a queued extraction without launching it", async () => {
    const { dequeueTransfer } = await import("./fileOps");
    extract(); extract();
    const [first, second] = useTransfers.getState().transfers;
    dequeueTransfer(second.id);
    done(first.id);
    expect(extracted).toEqual([first.id]);
  });
  it("releases the queue while an extraction waits for a password", async () => {
    const { handleExtractDone } = await import("./fileOps");
    extract(); extract();
    const [first, second] = useTransfers.getState().transfers;
    const error = { version: 1, id: "password", code: "archive.passwordRequired", params: {}, op: null, provider: null, retryable: false, action: null, detail: null };
    expect(handleExtractDone({ id: first.id, op: "extract", ok: 0, errors: [{ item: "source.zip", error }], cancelled: false, mode: "safe" })).toBe(true);
    expect(extracted).toEqual([first.id, second.id]);
    expect(useOps.getState().prompt?.password).toBe(true);
    useOps.getState().closeDialogs();
    done(second.id);
  });
});
