// ============================================================
// The same cases against both adapters (TICKET-035).
//
// The IPC client answers in two modes — a Tauri runtime, and the
// browser demo. Both are real, and the risk the split exists to manage
// is that they drift: a demo reply that no longer matches the typed
// contract fails only in the browser build, where nothing is watching.
//
// So the cases below are written once and run twice. What is asserted
// is what both modes must agree on: the shape of the answer, and the
// fact that a command exists at all. What each mode does *with* it
// differs by design and is asserted separately.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import { useDemo, useTauri } from "@/test/tauri";
import type { CommandImpl } from "@/test/tauri";

/** Fresh module instance per case: the client holds module state. */
async function client() {
  vi.resetModules();
  return import("./client");
}

/** What the mocked backend answers, mirroring the Rust return types. */
const NATIVE: Record<string, CommandImpl> = {
  list_dir: () => ({
    path: "/Users/demo",
    entries: [
      {
        name: "a.txt",
        is_dir: false,
        is_symlink: false,
        size: 1,
        modified: 0,
        mode: null,
        hidden: false,
        readonly: false,
        executable: false,
      },
    ],
  }),
  list_drives: () => [{ name: "/", mount: "/", total: 1, free: 1 }],
  home_dir: () => "/Users/demo",
  location_capabilities: () => ({
    scheme: "local",
    store_id: "local",
    connected: true,
    caps: ["browse"],
  }),
  volume_info: () => ({
    name: "disk",
    mount: "/",
    fs: "apfs",
    total: 10,
    used: 4,
    free: 6,
    used_pct: 40,
    free_pct: 60,
  }),
  file_props: () => ({
    path: "/Users/demo/a.txt",
    name: "a.txt",
    is_dir: false,
    is_symlink: false,
    size: 1,
    modified: 0,
    mode: null,
    uid: null,
    gid: null,
    owner: null,
    group: null,
    hidden: false,
    readonly: false,
    executable: false,
    xattrs: [],
    acl: [],
    unix: true,
  }),
  get_tags: () => [],
  net_list_profiles: () => [],
  net_list_connections: () => [],
  net_cred_status: () => ({
    config: {
      backend: "keychain",
      kdbx_path: "",
      key_file: "",
      master_in_keychain: false,
    },
    needs_unlock: false,
    unlocked: true,
  }),
  read_preview: () => ({ kind: "text", name: "a.txt", size: 1, text: "x" }),
  compare_levels: () => [],
};

/** Both modes, as a parameter. */
const MODES = [
  ["demo", () => useDemo()],
  ["tauri", () => void useTauri({ commands: NATIVE })],
] as const;

beforeEach(() => useDemo());

describe.each(MODES)("in %s mode", (_name, select) => {
  it("lists a directory with entries of the generated shape", async () => {
    select();
    const listing = await (await client()).listDir("/Users/demo");
    expect(typeof listing.path).toBe("string");
    expect(Array.isArray(listing.entries)).toBe(true);
    for (const e of listing.entries) {
      // Exactly the fields `DirEntry` declares — a demo that forgets one
      // would render a blank column instead of failing.
      expect(Object.keys(e).sort()).toEqual(
        [
          "executable",
          "hidden",
          "is_dir",
          "is_symlink",
          "modified",
          "mode",
          "name",
          "readonly",
          "size",
        ].sort(),
      );
    }
  });

  it("answers home_dir with a path", async () => {
    select();
    expect(typeof (await (await client()).homeDir())).toBe("string");
  });

  it("answers capabilities with the cache identity and a capability list", async () => {
    select();
    const caps = await (await client()).locationCapabilities("/Users/demo");
    expect(typeof caps.scheme).toBe("string");
    expect(typeof caps.store_id).toBe("string");
    expect(typeof caps.connected).toBe("boolean");
    expect(Array.isArray(caps.caps)).toBe(true);
  });

  it("answers volume info with totals that add up", async () => {
    select();
    const vol = await (await client()).volumeInfo("/Users/demo");
    expect(vol).not.toBeNull();
    expect(vol!.used + vol!.free).toBe(vol!.total);
  });

  it("answers file properties with the entry's own name", async () => {
    select();
    const props = await (await client()).fileProps("/Users/demo/a.txt");
    expect(props.name).toBe("a.txt");
    expect(typeof props.is_dir).toBe("boolean");
  });

  it("answers the credential status with a backend and a lock state", async () => {
    select();
    const cred = await (await client()).netCredStatus();
    // `backend` sits inside `config` — the generated shape, which is
    // exactly the kind of detail a hand-written demo reply gets wrong.
    expect(typeof cred.config.backend).toBe("string");
    expect(typeof cred.unlocked).toBe("boolean");
    expect(typeof cred.needs_unlock).toBe("boolean");
  });

  it("lists profiles and connections as arrays", async () => {
    select();
    const c = await client();
    expect(Array.isArray(await c.netListProfiles())).toBe(true);
    expect(Array.isArray(await c.netListConnections())).toBe(true);
  });

  it("returns a preview with a kind the dialog can switch on", async () => {
    select();
    const preview = await (await client()).readPreview("/Users/demo/a.txt", 1024);
    expect(["text", "image", "binary", "markdown", "code"]).toContain(
      preview.kind,
    );
  });

  it("compares two levels and returns a list", async () => {
    select();
    expect(Array.isArray(await (await client()).compareLevels([], []))).toBe(
      true,
    );
  });
});

describe("where the two modes differ, on purpose", () => {
  it("invokes a command with a runtime and none without", async () => {
    const tauri = useTauri({ commands: NATIVE });
    await (await client()).homeDir();
    expect(tauri.calls.map((c) => c.cmd)).toEqual(["home_dir"]);

    useDemo();
    // No runtime: the demo answers and nothing is invoked. A command
    // reaching `invoke` here would throw, which is the point.
    expect(await (await client()).homeDir()).toBe("/Users/demo");
  });

  it("makes fire-and-forget commands no-ops in the demo", async () => {
    useDemo();
    const c = await client();
    await expect(c.netDisconnect("p1")).resolves.toBeUndefined();
    await expect(c.netCancelConnect("conn-1")).resolves.toBeUndefined();
    await expect(c.refreshSubscribe([{id:"tab",generation:0,path:"/a"}])).resolves.toBeUndefined();
    expect(() => c.cancelTransfer("t1")).not.toThrow();
  });
});
