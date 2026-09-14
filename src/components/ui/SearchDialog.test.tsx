import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import axe from "axe-core";
import type { SearchHit, SearchSummary } from "@/ipc/bindings";
import { useTauri } from "@/test/tauri";
import { file } from "@/test/fixtures";
import { usePanes } from "@/store/panesStore";
import { useSearchDialog } from "@/store/searchStore";
import { useCaps } from "@/store/capsStore";
import { translate, useSettings } from "@/store/settingsStore";
import { useKeyboard } from "@/features/commander/useKeyboard";
import { SearchDialog } from "./SearchDialog";

vi.mock("@/ipc/invoke", async importOriginal => ({
  ...await importOriginal<typeof import("@/ipc/invoke")>(), hasTauri: true,
}));
// Only virtual measurements are unavailable in jsdom.
vi.mock("@tanstack/react-virtual", () => ({ useVirtualizer: ({ count }: { count: number }) => ({
  getVirtualItems: () => Array.from({ length: count }, (_, index) => ({ key: index, index, start: index * 40 })),
  getTotalSize: () => count * 40, scrollToIndex: vi.fn(),
}) }));

function Host() {
  useKeyboard();
  return <><button>{translate("search.title")}</button><SearchDialog /></>;
}

it.each(["de", "en"] as const)("searches and reveals a keyboard-selected streamed hit in %s", async language => {
  const hits: SearchHit[] = ["alpha.txt", "beta.txt"].map(name => ({ name, path: `/src/nested/${name}`,
    is_dir: false, size: 10, modified: 0, detail: "", group: 0 }));
  const runtime = useTauri({ commands: {
    location_capabilities: () => ({ scheme: "local", store_id: "local", connected: true, caps: ["browse", "search"] }),
    search: ({ onBatch }) => {
      const channel = onBatch as { onmessage: (batch: SearchHit[]) => void };
      channel.onmessage([hits[0]]);
      channel.onmessage([hits[1]]);
      return { status: "complete", hits: 2, files: 2, dirs: 0, unreadable: 0 };
    },
    list_dir: ({ path }) => ({ path, entries: hits.map(hit => file(hit.name)) }),
    request_git_status: () => {},
  } });
  useSettings.setState({ language });
  useCaps.setState({ known: { local: ["browse", "search"] } });
  usePanes.setState(s => ({ active: "left", left: { ...s.left,
    tabs: [{ ...s.left.tabs[0], path: "/src", archive: null }], activeTab: 0 } }));
  render(<Host />);
  const trigger = screen.getByRole("button", { name: translate("search.title") });
  trigger.focus();
  const user = userEvent.setup();
  await user.keyboard("{Alt>}{F7}{/Alt}");
  expect(screen.getByRole("dialog", { name: translate("search.title") })).toBeVisible();
  const nameInput = screen.getByRole("textbox", { name: translate("search.name") });
  // Traverse the real modal tab order, with a finite bound to catch a trap.
  for (let i = 0; i < 15 && document.activeElement !== nameInput; i++) await user.tab();
  expect(nameInput).toHaveFocus();
  await user.keyboard("*.txt{Enter}");
  await screen.findByRole("option", { name: /beta.txt/ });
  expect(runtime.calls.find(call => call.cmd === "search")?.args).toMatchObject({ root: "/src", options: { name: "*.txt", mode: "files" } });
  const list = screen.getByRole("listbox", { name: translate("search.results") });
  for (let i = 0; i < 15 && document.activeElement !== list; i++) await user.tab();
  expect(list).toHaveFocus();
  await user.keyboard("{End}");
  const selected = screen.getByRole("option", { name: /beta.txt/ });
  expect(selected).toHaveAttribute("aria-selected", "true");
  expect(selected).toHaveAttribute("aria-posinset", "2");
  expect(selected).toHaveAttribute("aria-setsize", "2");
  expect(list).toHaveAttribute("aria-activedescendant", selected.id);
  const report = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(report.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
  await user.keyboard("{Enter}");
  await waitFor(() => expect(usePanes.getState().left.tabs[0].path).toBe("/src/nested"));
  await waitFor(() => {
    const pane = usePanes.getState().left.tabs[0];
    expect(pane.entries[pane.cursor]?.name).toBe("beta.txt");
    expect(trigger).toHaveFocus();
  });
  expect(useSearchDialog.getState().side).toBeNull();
  expect(runtime.calls.filter(call => call.cmd === "search")).toHaveLength(1);
});

for (const status of ["complete", "cancelled"] as const) {
  it.each(["de", "en"] as const)(`announces ${status} without reading every streamed batch in %s`, async language => {
    useSettings.setState({ language });
    let send: ((batch: SearchHit[]) => void) | undefined;
    let finish: ((summary: SearchSummary) => void) | undefined;
    const done = new Promise<SearchSummary>(resolve => { finish = resolve; });
    const runtime = useTauri({ commands: {
      location_capabilities: () => ({ scheme: "local", store_id: "local", connected: true, caps: ["search"] }),
      search: ({ onBatch }) => { send = (onBatch as { onmessage: (batch: SearchHit[]) => void }).onmessage; return done; },
      cancel_search: () => {},
    } });
    usePanes.setState(s => ({ left: { ...s.left, tabs: [{ ...s.left.tabs[0], path: "/src" }], activeTab: 0 } }));
    useSearchDialog.getState().open("left");
    render(<SearchDialog />);
    const region = screen.getByRole("status");
    expect(region).toBeEmptyDOMElement();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: translate("search.run") }));
    await waitFor(() => expect(send).toBeDefined());
    expect(region).toHaveTextContent(translate("search.running"));
    const mutations: MutationRecord[] = [];
    const observer = new MutationObserver(records => mutations.push(...records));
    observer.observe(region, { childList: true, subtree: true, characterData: true });
    try {
      for (let index = 0; index < 3; index++) {
        await act(async () => send?.([{ name: `${index}.txt`, path: `/src/${index}.txt`,
          is_dir: false, size: 1, modified: 0, detail: "", group: 0 }]));
      }
      expect(screen.getAllByRole("option")).toHaveLength(3);
      expect(mutations).toHaveLength(0);
      if (status === "cancelled") {
        await user.click(screen.getByRole("button", { name: translate("search.stop") }));
        expect(runtime.calls.filter(call => call.cmd === "cancel_search")).toHaveLength(1);
      }
      await act(async () => finish?.({ status, hits: 3, files: 3, dirs: 0, unreadable: 0 }));
      expect(region).toHaveTextContent(status === "complete"
        ? translate("search.complete").replace("{n}", "3")
        : `${translate("search.cancelled")}: 3 ${translate("search.results")}`);
      expect(mutations.length).toBeGreaterThan(0);
    } finally { observer.disconnect(); }
  });
}

it("retains the completed search over six real unmount/reopen cycles", async () => {
  const hit: SearchHit = {name:"kept.txt",path:"/src/kept.txt",is_dir:false,size:1,modified:0,detail:"",group:0};
  const runtime = useTauri({commands:{
    location_capabilities: () => ({scheme:"local",store_id:"local",connected:true,caps:["search"]}),
    search: ({onBatch}) => {
      (onBatch as {onmessage:(hits:SearchHit[])=>void}).onmessage([hit]);
      return {status:"complete",hits:1,files:1,dirs:0,unreadable:0};
    },
  }});
  usePanes.setState(s => ({left:{...s.left,tabs:[{...s.left.tabs[0],path:"/src"}],activeTab:0}}));
  useSearchDialog.getState().open("left");
  let view = render(<SearchDialog/>);
  const user = userEvent.setup();
  await user.type(screen.getByRole("textbox",{name:translate("search.name")}),"*.txt");
  await user.click(screen.getByRole("button",{name:translate("search.run")}));
  await screen.findByRole("option",{name:/kept.txt/});
  for(let cycle=0;cycle<6;cycle++) {
    await user.keyboard("{Escape}");
    view.unmount(); // same lifecycle as LazyFeature's delayed removal
    act(()=>useSearchDialog.getState().open("left"));
    view=render(<SearchDialog/>);
    expect(screen.getByRole("textbox",{name:translate("search.name")})).toHaveValue("*.txt");
    expect(screen.getByRole("option",{name:/kept.txt/})).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent(translate("search.complete").replace("{n}","1"));
  }
  expect(runtime.calls.filter(c=>c.cmd==="search")).toHaveLength(1);
  act(()=>useSearchDialog.getState().open("right"));
  expect(screen.queryByRole("option")).not.toBeInTheDocument();
  expect(screen.getByRole("status")).toBeEmptyDOMElement();
  expect(runtime.calls.filter(c=>c.cmd==="search")).toHaveLength(1);
});

it("retains cancelled hits and ignores late batches and completion after reopening", async () => {
  const runs: {send:(hits:SearchHit[])=>void;finish:(summary:SearchSummary)=>void}[]=[];
  useTauri({commands:{
    location_capabilities:()=>({scheme:"local",store_id:"local",connected:true,caps:["search"]}),
    search:({onBatch})=>new Promise<SearchSummary>(finish=>runs.push({send:(onBatch as {onmessage:(hits:SearchHit[])=>void}).onmessage,finish})),
    cancel_search:()=>{},
  }});
  useSearchDialog.getState().open("left");
  usePanes.setState(s => ({left:{...s.left,tabs:[{...s.left.tabs[0],path:"/src"}],activeTab:0}}));
  useSearchDialog.getState().open("left");
  render(<SearchDialog/>);
  const user=userEvent.setup();
  await user.click(screen.getByRole("button",{name:translate("search.run")}));
  await waitFor(()=>expect(runs).toHaveLength(1));
  const hit:SearchHit={name:"partial.txt",path:"/partial.txt",is_dir:false,size:1,modified:0,detail:"",group:0};
  act(()=>runs[0].send([hit]));
  await user.keyboard("{Escape}");
  act(()=>useSearchDialog.getState().open("left"));
  expect(screen.getByRole("status")).toHaveTextContent(translate("search.cancelled"));
  expect(screen.getByRole("option",{name:/partial.txt/})).toBeVisible();
  await user.click(screen.getByRole("button",{name:translate("search.run")}));
  await waitFor(()=>expect(runs).toHaveLength(2));
  await act(async()=>{runs[0].send([hit]);runs[0].finish({status:"complete",hits:1,files:1,dirs:0,unreadable:0});});
  expect(screen.queryByRole("option")).not.toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent(translate("search.running"));
  await act(async()=>runs[1].finish({status:"complete",hits:0,files:0,dirs:0,unreadable:0}));
  expect(screen.getByRole("status")).toHaveTextContent(translate("search.complete").replace("{n}","0"));
});

it("keeps a failed search visible on reopen until parameters change", async () => {
  useTauri({commands:{location_capabilities:()=>({scheme:"local",store_id:"local",connected:true,caps:["search"]}),search:()=>Promise.reject("search failed")}});
  usePanes.setState(s=>({left:{...s.left,tabs:[{...s.left.tabs[0],path:"/src"}],activeTab:0}}));
  useSearchDialog.getState().open("left");
  let view=render(<SearchDialog/>);
  const user=userEvent.setup();
  await user.click(screen.getByRole("button",{name:translate("search.run")}));
  await screen.findByRole("alert");
  await user.keyboard("{Escape}");view.unmount();
  act(()=>useSearchDialog.getState().open("left"));view=render(<SearchDialog/>);
  expect(screen.getByRole("alert")).toHaveTextContent("search failed");
  await user.type(screen.getByRole("textbox",{name:translate("search.root")}),"/other");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("status")).toBeEmptyDOMElement();
});
