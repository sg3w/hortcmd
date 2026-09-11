// ============================================================
// IPC subscriptions, without a Tauri process (TICKET-033).
//
// Events are how the backend reports progress, external folder changes,
// terminal output and the connection lifecycle. Every one of those goes
// through the same shape: subscribe, fan out to all listeners, and
// return an unsubscribe that really removes one. A leak here is
// invisible — an unmounted view keeps reacting — so it is pinned down
// rather than observed.
//
// Both modes are exercised: the browser demo (no runtime) and a mocked
// Tauri runtime, which is what CI runs.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import { useDemo, useTauri, type InvokeCall } from "@/test/tauri";

/** Fresh module instance per test: the subscriber sets are module state. */
async function freshClient() {
  vi.resetModules();
  return import("./client");
}

beforeEach(() => {
  useDemo();
});

describe("with a Tauri runtime", () => {
  it("delivers one event to every listener", async () => {
    const tauri = useTauri();
    const client = await freshClient();

    const seen: string[] = [];
    client.onFsChanged((dir) => seen.push(`a:${dir}`));
    client.onFsChanged((dir) => seen.push(`b:${dir}`));
    // The listener is registered asynchronously (the Tauri core is
    // imported lazily on first use).
    await vi.waitFor(() => expect(tauri.calls).toBeDefined());
    await new Promise((r) => setTimeout(r, 0));

    tauri.emit("fs-changed", "/dir");
    expect(seen).toEqual(["a:/dir", "b:/dir"]);
  });

  it("removes exactly the listener that unsubscribed", async () => {
    const tauri = useTauri();
    const client = await freshClient();

    const seen: string[] = [];
    const off = client.onFsChanged((dir) => seen.push(`a:${dir}`));
    client.onFsChanged((dir) => seen.push(`b:${dir}`));
    await new Promise((r) => setTimeout(r, 0));

    off();
    tauri.emit("fs-changed", "/dir");
    expect(seen).toEqual(["b:/dir"]);

    // Unsubscribing twice is harmless.
    off();
    tauri.emit("fs-changed", "/again");
    expect(seen).toEqual(["b:/dir", "b:/again"]);
  });

  it("routes each event to its own subscribers and no others", async () => {
    const tauri = useTauri();
    const client = await freshClient();

    const changed: unknown[] = [];
    const done: unknown[] = [];
    const netState: unknown[] = [];
    client.onFsChanged((d) => changed.push(d));
    client.onFsDone((d) => done.push(d));
    client.onNetState((s) => netState.push(s));
    await new Promise((r) => setTimeout(r, 0));

    tauri.emit("fs-changed", "/dir");
    expect(changed).toHaveLength(1);
    expect(done).toHaveLength(0);
    expect(netState).toHaveLength(0);

    tauri.emit("net-state", []);
    expect(netState).toHaveLength(1);
    expect(changed).toHaveLength(1);
  });

  it("unwraps the git event into path and status", async () => {
    const tauri = useTauri();
    const client = await freshClient();

    const seen: [string, unknown][] = [];
    client.onGitStatusReady((path, git) => seen.push([path, git]));
    await new Promise((r) => setTimeout(r, 0));

    const status = { supported: true, entries: {} };
    tauri.emit("git-support-ready", { path: "/repo", status });
    expect(seen).toEqual([["/repo", status]]);
  });

  it("invokes the command a call maps to, with its arguments", async () => {
    const tauri = useTauri({
      commands: { net_cancel_connect: () => undefined },
    });
    const client = await freshClient();

    await client.netCancelConnect("conn-1");
    expect(tauri.calls).toEqual<InvokeCall[]>([
      { cmd: "net_cancel_connect", args: { attempt: "conn-1" } },
    ]);
  });

  it("surfaces a command that rejects", async () => {
    useTauri({
      commands: {
        net_cancel_connect: () => {
          throw new Error("kaputt");
        },
      },
    });
    const client = await freshClient();
    await expect(client.netCancelConnect("conn-1")).rejects.toThrow("kaputt");
  });
});

describe("in the browser demo", () => {
  it("invokes nothing at all", async () => {
    const client = await freshClient();
    // Demo mode has no runtime; a call that would need one is a no-op
    // rather than a failure, so the browser build stays usable.
    await expect(client.netCancelConnect("conn-1")).resolves.toBeUndefined();
    await expect(client.netDisconnect("p1")).resolves.toBeUndefined();
  });

  it("still accepts subscriptions and simply never fires them", async () => {
    const client = await freshClient();
    const seen: string[] = [];
    const off = client.onFsChanged((d) => seen.push(d));
    await new Promise((r) => setTimeout(r, 0));
    expect(seen).toEqual([]);
    expect(() => off()).not.toThrow();
  });

  it("answers from the demo data", async () => {
    const client = await freshClient();
    const listing = await client.listDir("/Users/demo");
    expect(listing.entries.length).toBeGreaterThan(0);
    expect(await client.homeDir()).toBe("/Users/demo");
  });
});
