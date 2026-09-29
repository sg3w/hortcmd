import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { row } from "@/test/fixtures";
import type { ItemError } from "@/ipc/bindings/ItemError";
const failure=(item:string,code:string):ItemError=>({item,error:{version:1,code,params:{},id:"rename-test",op:"rename",provider:null,retryable:false,action:null,detail:null}});
async function setup(errors:ItemError[],language:"de"|"en") {
  vi.resetModules();
  const harness=useTauri({commands:{rename_batch:()=>({ok:1,errors})}});
  const {usePanes}=await import("@/store/panesStore");
  const {useSettings,translate}=await import("@/store/settingsStore");
  const {useOps}=await import("@/store/opsStore");
  const {useRenameDialog}=await import("@/store/renameStore");
  const {RenameDialog}=await import("./RenameDialog");
  useSettings.setState({language});
  const refresh=vi.fn(async()=>{});
  usePanes.setState(s=>({refresh,left:{...s.left,tabs:[{...s.left.tabs[0],path:"/files",entries:[row("one.txt"),row("two.txt")],raw:[row("one.txt"),row("two.txt")],selected:new Set<string>()}],activeTab:0},right:{...s.right,tabs:[{...s.right.tabs[0],path:"/files"}],activeTab:0}}));
  useRenameDialog.getState().open("left");render(<RenameDialog/>);
  fireEvent.change(screen.getByPlaceholderText(translate("rename.name")),{target:{value:"[N]-renamed"}});
  fireEvent.click(screen.getByRole("button",{name:translate("rename.apply")}));
  await waitFor(()=>expect(useRenameDialog.getState().side).toBeNull());
  return {harness,refresh,useOps,translate};
}
it.each(["de","en"] as const)("keeps item names and translated causes on partial success in %s",async language=>{
  const {harness,refresh,useOps,translate}=await setup([failure("two.txt","fs.permission"),failure("three.txt","fs.notFound")],language);
  const message=useOps.getState().confirm?.message;
  expect(message).toContain("two.txt");expect(message).toContain(translate("error.fs.permission"));
  expect(message).toContain("three.txt");expect(message).toContain(translate("error.fs.notFound"));expect(message).not.toContain("[object Object]");
  expect(refresh.mock.calls).toEqual([["left"],["right"]]);
  expect(harness.calls.find(c=>c.cmd==="rename_batch")?.args).toEqual({dir:"/files",renames:[["one.txt","one-renamed.txt"],["two.txt","two-renamed.txt"]]});
});
it.each([{errors:[]},{errors:[failure("one.txt","cancelled")]}])("does not open an empty error confirmation",async ({errors})=>{
  const {useOps,refresh}=await setup(errors,"en");expect(useOps.getState().confirm).toBeNull();expect(refresh).toHaveBeenCalledTimes(2);
});

// The beta gate (TICKET-391): the dialog describes the undo contract only
// while file undo is on. Off, nothing is recorded, so there is nothing to
// describe — and a sentence about undo in a rename dialog that records none
// is worse than silence.
it.each([false, true])("shows the undo contract only while file undo is on (%s)", async on => {
  vi.resetModules();
  useTauri({ commands: { rename_batch: () => ({ ok: 0, errors: [] }) } });
  const { usePanes } = await import("@/store/panesStore");
  const { useSettings, translate } = await import("@/store/settingsStore");
  const { useRenameDialog } = await import("@/store/renameStore");
  const { RenameDialog } = await import("./RenameDialog");
  useSettings.setState({ language: "de", undoEnabled: on });
  usePanes.setState(s => ({ left: { ...s.left, tabs: [{ ...s.left.tabs[0], path: "/files", entries: [row("one.txt")], raw: [row("one.txt")], selected: new Set<string>() }], activeTab: 0 } }));
  useRenameDialog.getState().open("left");
  render(<RenameDialog />);
  const sentence = screen.queryByText(translate("undo.conditional"));
  expect(on ? sentence : sentence === null).toBeTruthy();
});
