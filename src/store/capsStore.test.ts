// ============================================================
// Capability cache and gating in the frontend (TICKET-031).
//
// The backend states what a location supports; these tests pin down
// what the UI does with that answer — one IPC call per location rather
// than per menu item, an empty set for a dropped connection, and a
// refusal that never reaches the backend.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Capability, LocationCapabilities } from "@/ipc/bindings";

// The client is mocked per test file: these tests are about the cache
// and the gate, not about the IPC transport.
const invoked: string[] = [];
let answers: Record<string, LocationCapabilities> = {};

vi.mock("@/ipc/client", () => ({
  locationCapabilities: (address: string) => {
    invoked.push(address);
    const key = keyOf(address);
    const found = answers[key];
    return found
      ? Promise.resolve(found)
      : Promise.reject(new Error(`Unbekanntes Adressschema: ${address}`));
  },
}));

function keyOf(address: string): string {
  const m = /^([a-z][a-z0-9+.-]*):\/\/([^/]*)/i.exec(address);
  if (!m) return "local";
  const scheme = m[1].toLowerCase();
  return scheme === "file" || scheme === "local"
    ? "local"
    : `${scheme}://${m[2].toLowerCase()}`;
}

const LOCAL_CAPS: Capability[] = [
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

const REMOTE_CORE: Capability[] = [
  "browse",
  "preview",
  "copyFrom",
  "copyTo",
  "move",
  "create",
  "deletePermanent",
];

function answer(
  key: string,
  scheme: string,
  storeId: string,
  caps: Capability[],
  connected = true,
): void {
  answers[key] = { scheme, store_id: storeId, connected, caps };
}

const { allows, allowsPair, capsOf, loadCaps, locationKey, useCaps } =
  await import("./capsStore");

beforeEach(() => {
  invoked.length = 0;
  answers = {};
  answer("local", "local", "local", LOCAL_CAPS);
  answer("sftp://u@host:22", "sftp", "sftp:u@host:22", [
    ...REMOTE_CORE,
    "sshTerminal",
  ]);
  answer("ftp://u@files:21", "ftp", "ftp:u@files:21", REMOTE_CORE);
  answer("smb://u@nas:445", "smb", "smb:u@nas:445", REMOTE_CORE);
  answer("webdav://u@dav:443", "webdav", "webdav:u@dav:443", REMOTE_CORE);
  answer("sftp://u@dead:22", "sftp", "sftp:u@dead:22", [], false);
  useCaps.getState().invalidateAll();
});

describe("locationKey", () => {
  it("maps every local spelling onto one key", () => {
    expect(locationKey("/Users/x")).toBe("local");
    expect(locationKey("C:\\Users\\x")).toBe("local");
    expect(locationKey("file:///Users/x")).toBe("local");
    expect(locationKey("local:///Users/x")).toBe("local");
  });

  it("canonicalizes hosts and ports while preserving username case", () => {
    expect(locationKey("sftp://u@Host:22/srv")).toBe("sftp:u@host:22");
    expect(locationKey("sftp://u@host/other/dir")).toBe("sftp:u@host:22");
    expect(locationKey("ftp://u@host:22/srv")).not.toBe("sftp:u@host:22");
    expect(locationKey("sftp://U@host/srv")).not.toBe(locationKey("sftp://u@host/srv"));
    expect(locationKey("sftp://u:secret@host/srv")).toBe("invalid");
  });
});

describe("cache", () => {
  it("asks the backend once per location, not once per question", async () => {
    // A menu asks a dozen questions about the same folder while it renders.
    for (const cap of LOCAL_CAPS) allows("/Users/x", cap);
    await loadCaps("/Users/x");
    expect(invoked.filter((a) => keyOf(a) === "local")).toHaveLength(1);
  });

  it("serves a second folder of the same server from the cache", async () => {
    await loadCaps("sftp://u@host:22/srv");
    invoked.length = 0;
    expect(allows("sftp://u@host:22/srv/deep/inside", "browse")).toBe(true);
    expect(invoked).toHaveLength(0);
  });

  it("answers 'nothing' until the backend has spoken", () => {
    expect(capsOf("/Users/x")).toHaveLength(0);
  });

  it("drops remote answers on a connection change but keeps the local one", async () => {
    await loadCaps("/Users/x");
    await loadCaps("sftp://u@host:22/srv");
    useCaps.getState().invalidateRemote();
    expect(allows("/Users/x", "trash")).toBe(true);
    expect(allows("sftp://u@host:22/srv", "browse")).toBe(false);
  });
});

describe("the initial matrix", () => {
  it("keeps the local features", async () => {
    await loadCaps("/Users/x");
    for (const cap of LOCAL_CAPS) {
      expect(allows("/Users/x", cap), cap).toBe(true);
    }
  });

  it("gives every connected remote provider the VFS core set", async () => {
    for (const addr of [
      "sftp://u@host:22/srv",
      "ftp://u@files:21/pub",
      "smb://u@nas:445/share",
      "webdav://u@dav:443/remote",
    ]) {
      await loadCaps(addr);
      for (const cap of REMOTE_CORE) {
        expect(allows(addr, cap), `${addr} ${cap}`).toBe(true);
      }
    }
  });

  it("withholds every local-only command from remote providers", async () => {
    const localOnly: Capability[] = [
      "trash",
      "systemOpen",
      "openWith",
      "quickLook",
      "localTerminal",
      "volumeInfo",
      "gitStatus",
      "finderTags",
      "search",
      "compareDirs",
      "compareFiles",
      "archiveBrowse",
      "archiveExtractFrom",
      "archivePackFrom",
      "checksums",
      "recursiveSize",
      "stat",
      "permissions",
    ];
    for (const addr of [
      "sftp://u@host:22/srv",
      "ftp://u@files:21/pub",
      "smb://u@nas:445/share",
      "webdav://u@dav:443/remote",
    ]) {
      await loadCaps(addr);
      for (const cap of localOnly) {
        expect(allows(addr, cap), `${addr} ${cap}`).toBe(false);
      }
    }
  });

  it("offers the SSH terminal on SFTP and nowhere else", async () => {
    await loadCaps("sftp://u@host:22/srv");
    expect(allows("sftp://u@host:22/srv", "sshTerminal")).toBe(true);
    for (const addr of [
      "ftp://u@files:21/pub",
      "smb://u@nas:445/share",
      "webdav://u@dav:443/remote",
      "/Users/x",
    ]) {
      await loadCaps(addr);
      expect(allows(addr, "sshTerminal"), addr).toBe(false);
    }
  });

  it("gives a disconnected location nothing at all", async () => {
    await loadCaps("sftp://u@dead:22/srv");
    for (const cap of [...REMOTE_CORE, "sshTerminal" as Capability]) {
      expect(allows("sftp://u@dead:22/srv", cap), cap).toBe(false);
    }
  });

  it("treats an unknown scheme and a malformed address as 'nothing'", async () => {
    expect(await loadCaps("gopher://host/x")).toHaveLength(0);
    expect(await loadCaps("sftp://")).toHaveLength(0);
  });
});

describe("two-location operations", () => {
  const combos: [string, string, boolean][] = [
    ["/a", "/b", true],
    ["/a", "sftp://u@host:22/srv", false],
    ["sftp://u@host:22/srv", "/b", false],
    ["sftp://u@host:22/srv", "ftp://u@files:21/pub", false],
  ];

  it("asks each end for its own half of a pack", async () => {
    for (const [from, to, expected] of combos) {
      await loadCaps(from);
      await loadCaps(to);
      expect(
        allowsPair(from, "archivePackFrom", to, "archivePackTo"),
        `${from} → ${to}`,
      ).toBe(expected);
    }
  });

  it("allows a copy across providers where both ends support their half", async () => {
    for (const [from, to] of combos) {
      await loadCaps(from);
      await loadCaps(to);
      expect(allowsPair(from, "copyFrom", to, "copyTo"), `${from} → ${to}`).toBe(
        true,
      );
    }
  });

  it("refuses a copy as soon as one end is disconnected", async () => {
    await loadCaps("/a");
    await loadCaps("sftp://u@dead:22/srv");
    expect(allowsPair("/a", "copyFrom", "sftp://u@dead:22/srv", "copyTo")).toBe(
      false,
    );
    expect(allowsPair("sftp://u@dead:22/srv", "copyFrom", "/a", "copyTo")).toBe(
      false,
    );
  });
});
