import { beforeEach, expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { file } from "@/test/fixtures";
import type { DirListing } from "@/ipc/bindings";

function deferred() {
  let resolve!: (value: DirListing) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<DirListing>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
let calls: ReturnType<typeof useTauri>["calls"];
let pending: Map<string, ReturnType<typeof deferred>>;
let panes: typeof import("../panesStore");
beforeEach(async () => {
  vi.resetModules();
  pending = new Map();
  ({ calls } = useTauri({ commands: {
    list_dir: ({ path }) => {
      const request = deferred();
      pending.set(String(path), request);
      return request.promise;
    },
    home_dir: () => "/home",
    location_capabilities: () => ({scheme:"local", store_id:"local", connected:true, caps:[]}),
  }}));
  panes = await import("../panesStore");
});
async function request(path: string) {
  await vi.waitFor(() => expect(pending.has(path)).toBe(true));
  return pending.get(path)!;
}
const listing = (path: string) => ({path, entries:[file(`${path.slice(1)}.txt`)]});

it("ignores a navigation response that arrives after a newer destination", async () => {
  const { usePanes, panelOf } = panes;
  const first = usePanes.getState().loadDir("left", "/first");
  const second = usePanes.getState().loadDir("left", "/second");
  (await request("/second")).resolve(listing("/second")); await second;
  (await request("/first")).resolve(listing("/first")); await first;
  expect(panelOf(usePanes.getState(), "left").path).toBe("/second");
  expect(panelOf(usePanes.getState(), "left").history).toEqual(["/second"]);
});

it("delivers a result to its original tab after switching and closing other tabs", async () => {
  const { usePanes } = panes;
  const first = usePanes.getState().loadDir("left", "/first");
  const second = usePanes.getState().addTab("left", "/second");
  (await request("/second")).resolve(listing("/second")); await second;
  usePanes.getState().closeTab("left", 1);
  (await request("/first")).resolve(listing("/first")); await first;
  expect(usePanes.getState().left.tabs.map(t => t.path)).toEqual(["/first"]);
});

it("does not resurrect a tab closed while its listing was pending", async () => {
  const { usePanes } = panes;
  const load = usePanes.getState().addTab("left", "/closed");
  usePanes.getState().closeTab("left", 1);
  (await request("/closed")).resolve(listing("/closed")); await load;
  expect(usePanes.getState().left.tabs).toHaveLength(1);
  expect(usePanes.getState().left.tabs[0].path).toBe("");
});

it("ignores a failed older request and retains existing rows on current refresh failure", async () => {
  const { usePanes, panelOf } = panes;
  const first = usePanes.getState().loadDir("left", "/old");
  const second = usePanes.getState().loadDir("left", "/current");
  (await request("/current")).resolve(listing("/current")); await second;
  (await request("/old")).reject(new Error("old failure")); await first;
  expect(panelOf(usePanes.getState(), "left").loadError).toBeNull();
  pending.delete("/current");
  const refresh = usePanes.getState().refresh("left");
  const current = await request("/current");
  expect(panelOf(usePanes.getState(), "left").freshness).toBe("refreshing");
  expect(calls[calls.length - 1]).toMatchObject({ cmd: "list_dir", args: { fresh: true } });
  current.reject(new Error("offline")); await refresh;
  const tab = panelOf(usePanes.getState(), "left");
  expect(tab.loadError).toContain("offline");
  expect(tab.freshness).toBe("stale");
  expect(tab.raw[0].name).toBe("current.txt");
});

it("does not replace navigation performed while a saved session was restoring", async () => {
  const { usePanes, panelOf } = panes;
  const restore = usePanes.getState().restoreSession({
    left:{tabs:[{path:"/saved-left"}],activeTab:0,viewMode:"details"},
    right:{tabs:[{path:"/saved-right"}],activeTab:0,viewMode:"details"},active:"right",
  });
  await request("/saved-left");
  const navigation = usePanes.getState().loadDir("left", "/chosen");
  (await request("/chosen")).resolve(listing("/chosen")); await navigation;
  (await request("/saved-left")).resolve(listing("/saved-left"));
  (await request("/saved-right")).resolve(listing("/saved-right")); await restore;
  expect(panelOf(usePanes.getState(), "left").path).toBe("/chosen");
  expect(panelOf(usePanes.getState(), "right").path).toBe("/saved-right");
  expect(usePanes.getState().active).toBe("left");
});
