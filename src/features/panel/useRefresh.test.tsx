import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { file } from "@/test/fixtures";
import type { RefreshBatch } from "@/ipc/bindings/RefreshBatch";
import type { DirListing } from "@/ipc/bindings";

let panes: typeof import("@/store/panesStore");
let hook: typeof import("./useRefresh");
let tauri: ReturnType<typeof useTauri>;
let batch: RefreshBatch;
beforeEach(async () => {
  vi.resetModules();
  Object.defineProperty(document, "visibilityState", {configurable:true,value:"visible"});
  batch = {status:[],work:[]};
  tauri = useTauri({commands:{
    refresh_subscribe: () => {},
    refresh_poll: () => { const next = batch; batch = {status:[],work:[]}; return next; },
    refresh_finish: () => {},
  }});
  panes = await import("@/store/panesStore");
  hook = await import("./useRefresh");
  const { newTab } = await import("@/store/panes/state");
  panes.usePanes.setState(state => ({left:{...state.left,tabs:[newTab("/dir")]}, right:{...state.right,tabs:[newTab("/dir")]}}));
});
afterEach(() => { Object.defineProperty(document, "visibilityState", {configurable:true,value:"visible"}); });

it("applies one shared read while preserving each tab's selection and cursor by name", async () => {
  let finish!: (listing: DirListing) => void;
  tauri.setCommand("refresh_read", () => new Promise<DirListing>(resolve => {finish = resolve;}));
  const { usePanes } = panes;
  usePanes.setState(state => ({
    left:{...state.left,tabs:[{...state.left.tabs[0],entries:[file("b")],cursor:0,selected:new Set(["b"])}]},
    right:{...state.right,tabs:[{...state.right.tabs[0],entries:[file("c")],cursor:0,selected:new Set(["c"])}]},
  }));
  const targets = [usePanes.getState().left.tabs[0], usePanes.getState().right.tabs[0]].map(t => ({id:t.id,generation:t.request,path:t.path}));
  batch = {status:[],work:[{key:"/dir",serial:1,path:"/dir",targets}]};
  const mounted = renderHook(() => hook.useRefresh());
  await waitFor(() => expect(finish).toBeTypeOf("function"));
  await act(async () => { finish({path:"/dir",entries:[file("a"),file("b"),file("c")]}); });
  await waitFor(() => expect(tauri.calls.some(c => c.cmd === "refresh_finish")).toBe(true));
  for (const [side, name] of [["left","b"],["right","c"]] as const) {
    const tab = usePanes.getState()[side].tabs[0];
    expect([...tab.selected]).toEqual([name]);
    expect(tab.entries[tab.cursor].name).toBe(name);
  }
  expect(tauri.calls.filter(c => c.cmd === "refresh_read")).toHaveLength(1);
  mounted.unmount();
  await waitFor(() => expect(tauri.calls.filter(c => c.cmd === "refresh_subscribe").slice(-1)[0]?.args.subscriptions).toEqual([]));
});

it("stops polling while hidden, resumes on visibility and tears down subscriptions", async () => {
  const mounted = renderHook(() => hook.useRefresh());
  await waitFor(() => expect(tauri.calls.some(c => c.cmd === "refresh_poll")).toBe(true));
  await act(async () => {
    Object.defineProperty(document, "visibilityState", {configurable:true,value:"hidden"});
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await waitFor(() => expect(tauri.calls.filter(c => c.cmd === "refresh_poll").slice(-1)[0]?.args.visible).toBe(false));
  const count = tauri.calls.filter(c => c.cmd === "refresh_poll").length;
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 350)); });
  expect(tauri.calls.filter(c => c.cmd === "refresh_poll")).toHaveLength(count);
  await act(async () => {
    Object.defineProperty(document, "visibilityState", {configurable:true,value:"visible"});
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await waitFor(() => expect(tauri.calls.filter(c => c.cmd === "refresh_poll").slice(-1)[0]?.args.visible).toBe(true));
  mounted.unmount();
  await waitFor(() => expect(tauri.calls.filter(c => c.cmd === "refresh_subscribe").slice(-1)[0]?.args.subscriptions).toEqual([]));
});

it("fetches the connection snapshot once for the whole app, not once per pane picker", async () => {
  tauri.setCommand("net_list_profiles", () => []);
  tauri.setCommand("net_list_connections", () => []);
  const mounted = renderHook(() => hook.useRefresh());
  await waitFor(() => expect(tauri.calls.some(c => c.cmd === "net_list_connections")).toBe(true));
  const { DriveSelect } = await import("./DriveSelect");
  const { render } = await import("@testing-library/react");
  const panels = render(<><DriveSelect side="left" drives={[]}/><DriveSelect side="right" drives={[]}/></>);
  await act(async () => { await Promise.resolve(); });
  expect(tauri.calls.filter(c => c.cmd === "net_list_profiles")).toHaveLength(1);
  expect(tauri.calls.filter(c => c.cmd === "net_list_connections")).toHaveLength(1);
  panels.unmount(); mounted.unmount();
});
