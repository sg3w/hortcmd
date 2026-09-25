import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { useCaps } from "@/store/capsStore";
import { expect, it, vi } from "vitest";
import type { DiffEntry } from "@/ipc/bindings";
import { useTauri } from "@/test/tauri";
import { useCompareDialog } from "@/store/compareStore";
import { usePanes } from "@/store/panesStore";
import { translate as t, useSettings } from "@/store/settingsStore";
import { CompareDialog } from "./CompareDialog";
vi.mock("@/ipc/invoke", async original=>({...await original<typeof import("@/ipc/invoke")>(),hasTauri: () => true}));
vi.mock("@tanstack/react-virtual",()=>({useVirtualizer:({count,getItemKey}:{count:number;getItemKey:(i:number)=>string})=>({getVirtualItems:()=>Array.from({length:count},(_,index)=>({index,key:getItemKey(index),start:index*28})),getTotalSize:()=>count*28,scrollToIndex:vi.fn()})}));
const pair=(rel:string,status="different",left=true,right=true):DiffEntry=>({rel,name:rel.split("/").pop()!,status,left,right,left_size:123,right_size:456,left_modified:1700000000,right_modified:1700001000});
const rows=[pair("src/Grüße app.ts","newer_left"),pair("left.txt","left_only",true,false),pair("right.txt","right_only",false,true),pair("equal.txt","same")];
function setup(data=rows,same=false) {
  const h=useTauri({commands:{
    location_capabilities:()=>({scheme:"local",store_id:"local",connected:true,caps:["compareDirs","copyFrom","copyTo"]}),
    compare_dirs:({onBatch})=>{(onBatch as {onmessage:(rows:DiffEntry[])=>void}).onmessage(data);return false;},
    sync_copy:()=>({ok:1,errors:[]}),list_dir:({path})=>({path,entries:[]}),
  }});
  useSettings.setState({gitEnabled:false});
  usePanes.setState(s=>({left:{...s.left,tabs:[{...s.left.tabs[0],path:"/left"}],activeTab:0},right:{...s.right,tabs:[{...s.right.tabs[0],path:same?"/left":"/right"}],activeTab:0}}));
  useCompareDialog.getState().setOpen(true);return h;
}
it.each(["de","en"] as const)("pairs both metadata sides and missing values in %s",async language=>{
  setup();useSettings.setState({language});render(<CompareDialog/>);
  const check=await screen.findByRole("checkbox",{name:"left.txt"});
  const cells=within(check.closest('[role="row"]') as HTMLElement).getAllByRole("gridcell");
  expect(cells).toHaveLength(8);
  expect(cells[1]).toHaveTextContent("left.txt");expect(cells[5]).toHaveTextContent("—");expect(cells[6]).toHaveTextContent("—");expect(cells[7]).toHaveTextContent("—");
  expect(cells[4]).toHaveAccessibleName(t("compare.status.left_only"));
  expect(screen.getByRole("textbox",{name:t("compare.left")})).toHaveValue("/left");
  expect(screen.getByRole("columnheader",{name:`${t("compare.right")} · ${t("compare.col.date")}`})).toBeInTheDocument();
  await userEvent.click(screen.getByRole("checkbox",{name:/^(Gleiche|Equal) /}));
  expect(screen.getByRole("checkbox",{name:"equal.txt"})).toBeInTheDocument();
  const report=await axe.run(screen.getByRole("dialog"),{rules:{"color-contrast":{enabled:false}}});
  expect(report.violations.filter(v=>v.impact==="serious"||v.impact==="critical")).toEqual([]);
});
it("removes hidden selection and copies only visible files with a source, then compares again",async()=>{
  const h=setup();render(<CompareDialog/>);await screen.findByRole("checkbox",{name:"left.txt"});
  await userEvent.click(screen.getByRole("checkbox",{name:t("compare.selectAll")}));
  await userEvent.click(screen.getByRole("checkbox",{name:/^Nur links /}));
  expect(screen.queryByRole("checkbox",{name:"left.txt"})).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button",{name:new RegExp(t("compare.toRight"))}));
  await waitFor(()=>expect(h.calls.filter(c=>c.cmd==="sync_copy")).toHaveLength(1));
  expect(h.calls.find(c=>c.cmd==="sync_copy")?.args.items).toEqual([["/left/src/Grüße app.ts","/right/src/Grüße app.ts"]]);
  await waitFor(()=>expect(h.calls.filter(c=>c.cmd==="compare_dirs")).toHaveLength(2));
});
it("disables same-root copies and exposes unknown metadata",async()=>{
  setup([{...pair("unknown.txt","same"),left_modified:0}],true);render(<CompareDialog/>);
  await screen.findByRole("checkbox",{name:"unknown.txt"});
  expect(screen.getByRole("gridcell",{name:t("compare.status.unknown")})).toHaveTextContent("?");
  await userEvent.click(screen.getByRole("checkbox",{name:t("compare.selectAll")}));
  expect(screen.getByRole("button",{name:new RegExp(t("compare.toRight"))})).toBeDisabled();
});
it("ignores stale batches after recursive changes and close/reopen",async()=>{
  const h=setup([]);const runs:{send:(rows:DiffEntry[])=>void;finish:(cut:boolean)=>void}[]=[];
  h.setCommand("compare_dirs",({onBatch})=>new Promise<boolean>(finish=>runs.push({send:(onBatch as {onmessage:(rows:DiffEntry[])=>void}).onmessage,finish})));
  render(<CompareDialog/>);await waitFor(()=>expect(runs).toHaveLength(1));
  await userEvent.click(screen.getByRole("checkbox",{name:t("compare.recursive")}));
  await waitFor(()=>expect(runs).toHaveLength(2));
  await act(async()=>{runs[0].send([pair("old")]);runs[0].finish(true);runs[1].send([pair("current")]);runs[1].finish(false);});
  expect(screen.queryByRole("checkbox",{name:"old"})).not.toBeInTheDocument();
  expect(screen.getByRole("checkbox",{name:"current"})).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button",{name:t("op.close")}));
  act(()=>useCompareDialog.getState().setOpen(true));await waitFor(()=>expect(runs).toHaveLength(3));
  act(()=>runs[1].send([pair("late")]));expect(screen.queryByRole("checkbox",{name:"late"})).not.toBeInTheDocument();
});
it("copies in the reverse direction and reports backend errors without an unhandled rejection",async()=>{
  const h=setup([pair("only-right","right_only",false,true)]);h.setCommand("sync_copy",()=>Promise.reject("copy failed"));
  render(<CompareDialog/>);await screen.findByRole("checkbox",{name:"only-right"});
  await userEvent.click(screen.getByRole("checkbox",{name:t("compare.selectAll")}));
  expect(screen.getByRole("button",{name:new RegExp(t("compare.toRight"))})).toBeDisabled();
  await userEvent.click(screen.getByRole("button",{name:new RegExp(t("compare.toLeft"))}));
  await screen.findByText("copy failed");
  expect(h.calls.find(c=>c.cmd==="sync_copy")?.args.items).toEqual([["/right/only-right","/left/only-right"]]);
});
it("shows limit and empty/error outcomes explicitly",async()=>{
  const h=setup([]);h.setCommand("compare_dirs",()=>true);render(<CompareDialog/>);
  await screen.findByText(t("compare.truncated"));expect(screen.getByRole("status")).toHaveTextContent(t("compare.noPairs"));
  h.setCommand("compare_dirs",()=>Promise.reject("unavailable"));
  await userEvent.click(screen.getByRole("button",{name:t("compare.refresh")}));
  expect(await screen.findByRole("alert")).toHaveTextContent("unavailable");
});
it("keeps keyboard selection on complete pairs",async()=>{
  setup();render(<CompareDialog/>);await screen.findByRole("checkbox",{name:"left.txt"});
  const grid=screen.getByRole("grid");grid.focus();await userEvent.keyboard("{End} ");
  const selected=screen.getByRole("checkbox",{name:"src/Grüße app.ts"});
  expect(selected).toBeChecked();expect(selected.closest('[role="row"]')).toHaveAttribute("aria-selected","true");
});

it("disables copying when capabilities disappear after comparison",async()=>{
  const h=setup();render(<CompareDialog/>);await screen.findByRole("checkbox",{name:"left.txt"});
  await userEvent.click(screen.getByRole("checkbox",{name:t("compare.selectAll")}));
  act(()=>useCaps.setState({known:{local:[]}}));
  expect(screen.getByRole("button",{name:new RegExp(t("compare.toRight"))})).toBeDisabled();
  expect(screen.getByText(t("compare.copyUnavailable"))).toBeVisible();
  expect(h.calls.filter(c=>c.cmd==="sync_copy")).toHaveLength(0);
});
