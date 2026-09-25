import { act, render, renderHook, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { ensureTauriEvents, drivesSubs } from "@/ipc/client";
import { useDriveState, useDrives } from "@/store/driveStore";
import { panelOf, usePanes } from "@/store/panesStore";
import { useDriveFallback } from "./useDriveFallback";
import { DriveSelect } from "./DriveSelect";
import { FileBrowserDialog } from "@/components/ui/FileBrowserDialog";
import { openFileBrowser } from "@/store/fileBrowserStore";
import { translate } from "@/store/settingsStore";
vi.mock("@/ipc/invoke", async importOriginal => ({ ...await importOriginal<typeof import("@/ipc/invoke")>(), hasTauri:true }));
beforeAll(() => { HTMLElement.prototype.scrollIntoView = vi.fn(); });
function runtime(options: Parameters<typeof useTauri>[0]) {
  return useTauri(options);
}
const root = {name:"Root", mount:"/", total:100, free:50};
const card = {name:"Card", mount:"/Volumes/card", total:100, free:80};
function setPath(path: string) {
  usePanes.setState(s => ({left:{...s.left,tabs:s.left.tabs.map((t,i) => i === s.left.activeTab ? {...t,path,archive:null,loading:false,request:t.request+1} : t)}}));
}
beforeEach(() => { useDriveState.setState({drives:[root],removed:[card]}); setPath("/Volumes/card/folder"); });
it("returns an affected panel home", async () => {
  const h = runtime({commands:{home_dir:()=>"/home",list_dir:({path})=>({path,entries:[]}),location_capabilities:()=>({scheme:"local",store_id:"local",connected:true,caps:[]})}});
  renderHook(() => useDriveFallback([root]));
  await waitFor(() => expect(panelOf(usePanes.getState(),"left").path).toBe("/home"));
  expect(h.calls.some(c=>c.cmd==="list_dir" && c.args.path==="/home")).toBe(true);
});
it("does not overwrite navigation while home lookup is pending", async () => {
  let finish!: (home:string)=>void;
  const h=runtime({commands:{home_dir:()=>new Promise<string>(resolve=>{finish=resolve;})}});
  renderHook(() => useDriveFallback([root]));
  await waitFor(()=>expect(finish).toBeTypeOf("function"));
  act(()=>setPath("/other"));
  await act(async()=>{finish("/home");});
  expect(h.calls.some(c=>c.cmd==="list_dir")).toBe(false);
});
it("removes a volume from an already open Radix picker", async () => {
  useDriveState.setState({drives:[root,card],removed:[]});
  runtime({commands:{list_drives:()=>[root,card],net_list_profiles:()=>[],net_list_connections:()=>[]}});
  function Picker(){return <DriveSelect side="left" drives={useDrives()}/>;}
  render(<Picker/>); await ensureTauriEvents();
  screen.getByRole("combobox",{name:translate("drive.label")}).focus();
  await userEvent.keyboard("{ArrowDown}");
  expect(screen.getByRole("option",{name:/Card/})).toBeInTheDocument();
  act(()=>drivesSubs.forEach(cb => cb([root])));
  await waitFor(()=>expect(screen.queryByRole("option",{name:/Card/})).not.toBeInTheDocument());
  expect(screen.getByRole("option",{name:/Root/})).toBeInTheDocument();
});
it("updates an open file browser and recovers its removed folder", async () => {
  useDriveState.setState({drives:[root,card],removed:[]});
  runtime({commands:{list_drives:()=>[root,card],home_dir:()=>"/home",list_dir:({path})=>({path,entries:[]})}});
  render(<FileBrowserDialog/>);
  act(()=>{openFileBrowser({selectMode:"folder",initialPath:"/Volumes/card/folder",onPick:()=>{}});});
  await screen.findByDisplayValue("/Volumes/card/folder"); await ensureTauriEvents();
  act(()=>drivesSubs.forEach(cb => cb([root])));
  await screen.findByDisplayValue("/home");
  expect(screen.queryByRole("option",{name:"Card"})).not.toBeInTheDocument();
});
