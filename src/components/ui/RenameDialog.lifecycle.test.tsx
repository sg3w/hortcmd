import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { row } from "@/test/fixtures";
import type { OpResult } from "@/ipc/bindings/OpResult";
async function open() {
  vi.resetModules();
  let resolve!:(value:OpResult)=>void;let reject!:(cause:unknown)=>void;
  const result=new Promise<OpResult>((yes,no)=>{resolve=yes;reject=no;});
  const h=useTauri({commands:{rename_batch:()=>result}});
  const {usePanes}=await import("@/store/panesStore");
  const {useRenameDialog}=await import("@/store/renameStore");
  const {useSettings,translate:t}=await import("@/store/settingsStore");
  const {RenameDialog}=await import("./RenameDialog");
  useSettings.setState({language:"en"});const refresh=vi.fn(async()=>{});
  usePanes.setState(s=>({refresh,left:{...s.left,tabs:[{...s.left.tabs[0],path:"/files",entries:[row()],raw:[row()],selected:new Set<string>()}],activeTab:0}}));
  useRenameDialog.getState().open("left");const rendered=render(<RenameDialog/>);
  fireEvent.change(screen.getByPlaceholderText(t("rename.name")),{target:{value:"[N]-new"}});
  return {h,resolve,reject,useRenameDialog,refresh,t,...rendered};
}
it("submits once while pending and keeps the plan editable after a rejection",async()=>{
  const s=await open();const button=screen.getByRole("button",{name:s.t("rename.apply")});
  fireEvent.click(button);fireEvent.click(button);
  expect(button).toBeDisabled();await waitFor(()=>expect(s.h.calls.filter(c=>c.cmd==="rename_batch")).toHaveLength(1));
  await act(async()=>s.reject({version:1,code:"fs.permission",params:{},id:"test",op:"rename",provider:null,retryable:false,action:null,detail:null}));
  expect(await screen.findByRole("alert")).toHaveTextContent(s.t("error.fs.permission"));
  expect(screen.getByPlaceholderText(s.t("rename.name"))).toHaveValue("[N]-new");expect(button).toBeEnabled();expect(s.refresh).not.toHaveBeenCalled();
  s.h.setCommand("rename_batch",()=>({ok:1,errors:[]}));fireEvent.click(button);
  await waitFor(()=>expect(s.useRenameDialog.getState().side).toBeNull());
});
it("refreshes completed changes but does not close a reopened dialog",async()=>{
  const s=await open();fireEvent.click(screen.getByRole("button",{name:s.t("rename.apply")}));
  act(()=>{s.useRenameDialog.getState().close();s.useRenameDialog.getState().open("left");});
  const revision=s.useRenameDialog.getState().revision;
  await act(async()=>s.resolve({ok:1,errors:[]}));
  expect(s.useRenameDialog.getState().revision).toBe(revision);expect(s.useRenameDialog.getState().side).toBe("left");expect(s.refresh).toHaveBeenCalledWith("left");
  expect(screen.getByPlaceholderText(s.t("rename.name"))).toHaveValue("[N]");
});
it("ignores a stale rejection after close and reopen",async()=>{
  const s=await open();fireEvent.click(screen.getByRole("button",{name:s.t("rename.apply")}));
  act(()=>{s.useRenameDialog.getState().close();s.useRenameDialog.getState().open("left");});
  await act(async()=>s.reject(new Error("transport lost")));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();expect(s.useRenameDialog.getState().side).toBe("left");
});
