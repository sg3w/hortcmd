// ============================================================
// Action gating across every surface (TICKET-031).
//
// One capability contract has to drive the command table (which the
// in-window menu, the native menu and the toolbar read), the context
// menu and the keyboard alike. These tests put a pane on a location,
// state what that location supports, and check what the surfaces then
// offer — for local, SFTP, FTP, SMB, WebDAV, archive, file and folder
// contexts, and for a connection that drops while a menu is open.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Capability, LocationCapabilities } from "@/ipc/bindings";

let answers: Record<string, Capability[]> = {};
const notices: { key: string; vars?: Record<string, unknown> }[] = [];
/** Every backend call an action actually made — must stay empty for a
 *  refused action, which is the point of the gate. */
const calls: string[] = [];

function keyOf(address: string): string {
  const m = /^([a-z][a-z0-9+.-]*):\/\/([^/]*)/i.exec(address);
  if (!m) return "local";
  const scheme = m[1].toLowerCase();
  return scheme === "file" || scheme === "local"
    ? "local"
    : `${scheme}://${m[2].toLowerCase()}`;
}

vi.mock("@/ipc/client", () => {
  const spy = (name: string) => (...args: unknown[]) => {
    calls.push(`${name}:${String(args[0])}`);
    return Promise.resolve(undefined as never);
  };
  return {
    hasTauri: false,
    locationCapabilities: (address: string): Promise<LocationCapabilities> => {
      const key = keyOf(address);
      const caps = answers[key];
      if (!caps) return Promise.reject(new Error("unbekannt"));
      return Promise.resolve({
        scheme: key === "local" ? "local" : key.split("://")[0],
        store_id: key,
        connected: caps.length > 0,
        caps,
      });
    },
    openPath: spy("openPath"),
    openWith: spy("openWith"),
    openTerminal: spy("openTerminal"),
    quickLook: spy("quickLook"),
    makeDir: spy("makeDir"),
    renameEntry: spy("renameEntry"),
    deleteEntries: spy("deleteEntries"),
    trashEntries: spy("trashEntries"),
    startTransfer: spy("startTransfer"),
    startExtract: spy("startExtract"),
    startPack: spy("startPack"),
    readPreview: spy("readPreview"),
    listDir: () => Promise.resolve({ path: "/", entries: [] }),
    listArchive: () => Promise.resolve({ path: "/", entries: [] }),
    requestGitStatus: spy("requestGitStatus"),
    homeDir: () => Promise.resolve("/Users/demo"),
    onNativeMenu: () => () => {},
    setMenuLanguage: () => Promise.resolve(),
    netListProfiles: () => Promise.resolve([]),
    netListConnections: () => Promise.resolve([]),
    sshClose: () => Promise.resolve(),
    cancelDirSize: () => Promise.resolve(),
    dirSize: () => Promise.resolve({ stats: null, cancelled: true }),
  };
});

const LOCAL: Capability[] = [
  "browse",
  "preview",
  "copyFrom",
  "copyTo",
  "move",
  "create",
  "deletePermanent",
  "trash",
  "archiveBrowse",
  "archiveExtractFrom",
  "archiveExtractTo",
  "archivePackFrom",
  "archivePackTo",
  "stat",
  "recursiveSize",
  "checksums",
  "permissions",
  "search",
  "compareDirs",
  "compareFiles",
  "gitStatus",
  "finderTags",
  "volumeInfo",
  "systemOpen",
  "openWith",
  "quickLook",
  "localTerminal",
];
const REMOTE: Capability[] = [
  "browse",
  "preview",
  "copyFrom",
  "copyTo",
  "move",
  "create",
  "deletePermanent",
];

const { loadCaps, useCaps } = await import("@/store/capsStore");
const { usePanes } = await import("@/store/panesStore");
const { useNotices } = await import("@/store/noticeStore");
const { useClipboard } = await import("@/store/clipboardStore");
const { commandState, runCommand } = await import("./commands");
const { guard, pairState } = await import("./guard");
const { runDelete, runMkdir, runQuickLook } = await import("./fileOps");
const { file } = await import("@/test/fixtures");

/** Puts a pane on `path` with one file under the cursor. */
function place(
  side: "left" | "right",
  path: string,
  opts: { archive?: string | null; dir?: boolean } = {},
): void {
  const entry = file("datei.txt", { is_dir: opts.dir ?? false, size: 10 });
  usePanes.setState((s) => ({
    [side]: {
      ...s[side],
      tabs: [
        {
          ...s[side].tabs[0],
          path,
          archive: opts.archive ?? null,
          raw: [entry],
          entries: [entry],
          cursor: 0,
          selected: new Set<string>(),
        },
      ],
      activeTab: 0,
    },
  }));
}

async function put(path: string, caps: Capability[]): Promise<void> {
  answers[keyOf(path)] = caps;
  await loadCaps(path);
}

beforeEach(async () => {
  answers = {};
  notices.length = 0;
  calls.length = 0;
  useCaps.getState().invalidateAll();
  useNotices.setState({ left: null, right: null });
  useClipboard.setState({ mode: null, sourcePath: "", names: [] });
  usePanes.setState({ active: "left" });
  await put("/local/a", LOCAL);
  await put("/local/b", LOCAL);
});

/** The reason a command gives right now, or null when it is enabled. */
function reasonOf(id: Parameters<typeof commandState>[0]): string | null {
  const st = commandState(id);
  return st.disabled ? (st.reason ?? "disabled") : null;
}

describe("the command table over a local pane", () => {
  beforeEach(() => {
    place("left", "/local/a");
    place("right", "/local/b");
  });

  it("keeps every local action available", () => {
    for (const id of [
      "file.view",
      "file.edit",
      "file.copy",
      "file.move",
      "file.rename",
      "file.mkdir",
      "file.delete",
      "file.pack",
      "file.props",
      "cmd.search",
      "cmd.terminal",
      "cmd.openFolder",
      "view.quickLook",
    ] as const) {
      expect(reasonOf(id), id).toBeNull();
    }
  });
});

describe.each([
  ["sftp", "sftp://u@host:22/srv", [...REMOTE, "sshTerminal" as Capability]],
  ["ftp", "ftp://u@files:21/pub", REMOTE],
  ["smb", "smb://u@nas:445/share", REMOTE],
  ["webdav", "webdav://u@dav:443/remote", REMOTE],
])("the command table over a %s pane", (scheme, path, caps) => {
  beforeEach(async () => {
    await put(path, caps);
    place("left", path);
    place("right", "/local/b");
  });

  it("keeps the VFS operations available", () => {
    for (const id of ["file.view", "file.rename", "file.mkdir"] as const) {
      expect(reasonOf(id), id).toBeNull();
    }
  });

  it("disables every local-only action with an explanation", () => {
    for (const id of [
      "cmd.openFolder",
      "view.quickLook",
      "cmd.search",
      "file.props",
      "file.pack",
    ] as const) {
      const st = commandState(id);
      if (st.hidden) continue; // Quick Look is macOS-only
      expect(st.disabled, id).toBe(true);
      expect(st.reason, id).toBeTruthy();
    }
  });

  it("offers the SSH terminal only where the connection carries one", () => {
    expect(reasonOf("net.terminal") === null).toBe(scheme === "sftp");
    expect(reasonOf("cmd.terminal") === null).toBe(scheme === "sftp");
  });

  it("allows a transfer to the local pane in both directions", () => {
    expect(reasonOf("file.copy")).toBeNull();
    expect(reasonOf("file.move")).toBeNull();
  });
});

describe("two-location commands", () => {
  it("refuses a pack whose destination cannot hold the archive", async () => {
    await put("sftp://u@host:22/srv", REMOTE);
    place("left", "/local/a");
    place("right", "sftp://u@host:22/srv");
    const st = commandState("file.pack");
    expect(st.disabled).toBe(true);
    expect(st.reason).toBe("menu.why.destUnsupported");
  });

  it("refuses a pack whose source cannot be read locally", async () => {
    await put("sftp://u@host:22/srv", REMOTE);
    place("left", "sftp://u@host:22/srv");
    place("right", "/local/b");
    const st = commandState("file.pack");
    expect(st.disabled).toBe(true);
    expect(st.reason).toBe("menu.why.unsupported");
  });

  it("names the destination when only that end is unsupported", async () => {
    await put("ftp://u@files:21/pub", REMOTE);
    expect(
      pairState("/local/a", "compareDirs", "ftp://u@files:21/pub", "compareDirs")
        .reason,
    ).toBe("menu.why.destUnsupported");
    expect(
      pairState("ftp://u@files:21/pub", "compareDirs", "/local/a", "compareDirs")
        .reason,
    ).toBe("menu.why.unsupported");
  });

  it("checks the clipboard's own source folder on paste", async () => {
    await put("sftp://u@dead:22/srv", []);
    place("left", "/local/a");
    useClipboard.setState({
      mode: "copy",
      sourcePath: "sftp://u@dead:22/srv",
      names: ["x"],
    });
    const st = commandState("file.clipPaste");
    expect(st.disabled).toBe(true);
    expect(st.reason).toBe("menu.why.offline");
  });
});

describe("archive context", () => {
  beforeEach(() => {
    place("left", "/local/a.zip", { archive: "/local/a.zip" });
    place("right", "/local/b");
  });

  it("keeps writing actions out of an archive", () => {
    for (const id of ["file.mkdir", "file.rename", "file.delete"] as const) {
      expect(reasonOf(id), id).toBe("menu.why.archive");
    }
  });
});

describe("keyboard and execution paths", () => {
  it("explains a refused shortcut instead of doing nothing", async () => {
    await put("ftp://u@files:21/pub", REMOTE);
    place("left", "ftp://u@files:21/pub");
    expect(guard("left", "ftp://u@files:21/pub", "search")).toBe(false);
    expect(useNotices.getState().left?.key).toBe("menu.why.unsupported");
    expect(useNotices.getState().left?.tone).toBe("error");
  });

  it("never reaches the backend for a refused action", async () => {
    await put("ftp://u@files:21/pub", REMOTE);
    place("left", "ftp://u@files:21/pub");
    // Quick Look falls back to "select the row" rather than erroring.
    expect(runQuickLook("left")).toBe(false);
    expect(calls).toHaveLength(0);
  });

  it("deletes permanently where the location has no trash", async () => {
    await put("smb://u@nas:445/share", REMOTE);
    place("left", "smb://u@nas:445/share");
    usePanes.setState((s) => ({
      left: {
        ...s.left,
        tabs: [{ ...s.left.tabs[0], selected: new Set(["datei.txt"]) }],
      },
    }));
    const { useOps } = await import("@/store/opsStore");
    runDelete("left");
    // The confirmation shown is the permanent one, not "moves to trash".
    expect(useOps.getState().confirm?.danger).toBe(true);
    useOps.getState().closeDialogs();
  });

  it("blocks a create on a location that cannot create", async () => {
    await put("sftp://u@dead:22/srv", []);
    place("left", "sftp://u@dead:22/srv");
    runMkdir("left");
    expect(useNotices.getState().left?.key).toBe("menu.why.offline");
    expect(calls).toHaveLength(0);
  });
});

describe("a connection that drops while a menu is open", () => {
  it("invalidates the captured state and blocks the execution", async () => {
    await put("sftp://u@host:22/srv", [...REMOTE, "sshTerminal"]);
    place("left", "sftp://u@host:22/srv");
    place("right", "/local/b");
    // Drawn while the session is live.
    expect(reasonOf("file.mkdir")).toBeNull();

    // …and gone by the time the item is chosen.
    answers[keyOf("sftp://u@host:22/srv")] = [];
    useCaps.getState().invalidateRemote();
    await loadCaps("sftp://u@host:22/srv");

    expect(reasonOf("file.mkdir")).toBe("menu.why.offline");
    runCommand("file.mkdir");
    expect(calls).toHaveLength(0);
  });

  it("keeps the local answer across the invalidation", async () => {
    await put("sftp://u@host:22/srv", REMOTE);
    place("left", "/local/a");
    useCaps.getState().invalidateRemote();
    expect(reasonOf("file.mkdir")).toBeNull();
  });
});
