import { createRef } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { row } from "@/test/fixtures";
import { newTab } from "@/store/panes/state";
import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useSettings, translate } from "@/store/settingsStore";
import { FileContextMenu } from "@/components/ui/FileContextMenu";
import { CommandBar } from "./CommandBar";
import { openOpposite } from "./navigate";

vi.mock("@/ipc/invoke", async original => ({...await original<typeof import("@/ipc/invoke")>(),hasTauri:true}));
function fixture(side: Side, path: string, archive: string | null = null) {
  const runtime=useTauri({commands:{
    list_dir:({path})=>({path,entries:[]}),
    list_archive:({inner})=>({path:inner,entries:[]}),
    location_capabilities:()=>({scheme:"local",store_id:"local",connected:true,caps:["browse"]}),
  }});
  const other: Side=side==="left"?"right":"left";
  useSettings.setState({gitEnabled:false});
  usePanes.setState(s=>({active:side,[side]:{...s[side],tabs:[{...newTab(path),archive}],activeTab:0},
    [other]:{...s[other],viewMode:"list",tabs:[{...newTab("/before"),filter:"old",selected:new Set(["old"]),sort:{key:"size",asc:false}},newTab("/untouched")],activeTab:0}}));
  return {runtime,other};
}
for(const side of ["left","right"] as const) {
  it.each(["/", "/Ordner mit Leerzeichen/Grüße", "sftp://user@host:2222/Ordner%23eins"])(`opens %s from ${side} without changing source, tabs or target sort`,async path=>{
    const {runtime,other}=fixture(side,path);
    const before=panelOf(usePanes.getState(),side);
    await openOpposite(side);
    expect(panelOf(usePanes.getState(),side)).toEqual(before);
    expect(usePanes.getState().active).toBe(side);
    expect(usePanes.getState()[other].tabs.map(t=>t.path)).toEqual([path,"/untouched"]);
    expect(panelOf(usePanes.getState(),other)).toMatchObject({sort:{key:"size",asc:false},filter:"",selected:new Set(),history:["/before",path]});
    await openOpposite(side);
    expect(panelOf(usePanes.getState(),other).history).toEqual(["/before",path]);
    usePanes.getState().goBack(other);
    await waitFor(()=>expect(panelOf(usePanes.getState(),other).path).toBe("/before"));
    expect(runtime.calls.every(c=>["list_dir","location_capabilities"].includes(c.cmd))).toBe(true);
  });
}
it("retains archive container and inner path",async()=>{
  const {runtime}=fixture("left","nested/Grüße","/packed file.zip");
  await openOpposite("left");
  expect(panelOf(usePanes.getState(),"right")).toMatchObject({archive:"/packed file.zip",path:"nested/Grüße"});
  expect(runtime.calls.find(c=>c.cmd==="list_archive")?.args).toMatchObject({archive:"/packed file.zip",inner:"nested/Grüße"});
});
it("reports a disconnected destination without changing source or old target path",async()=>{
  const {runtime}=fixture("left","sftp://user@host/folder");
  runtime.setCommand("list_dir",()=>Promise.reject("Disconnected"));
  await openOpposite("left");
  expect(panelOf(usePanes.getState(),"left").path).toBe("sftp://user@host/folder");
  expect(panelOf(usePanes.getState(),"right")).toMatchObject({path:"/before",loadError:"Disconnected",loading:false});
});
it.each([["de","dark"],["de","light"],["en","dark"],["en","light"]] as const)("toolbar exposes a localized keyboard-operable action in %s / %s",async(language,theme)=>{
  fixture("right","/source");useSettings.setState({language,theme});
  render(<CommandBar/>);
  const button=screen.getByRole("button",{name:translate("menu.nav.openOpposite")});
  button.focus();expect(button).toHaveFocus();
  await userEvent.setup().keyboard("{Enter}");
  await waitFor(()=>expect(panelOf(usePanes.getState(),"left").path).toBe("/source"));
  expect(usePanes.getState().active).toBe("right");
});
it.each([["/source",null],["sftp://user@host/root",null],["inner","/packed.zip"]] as const)("context menu targets the clicked subfolder at %s",async(path,archive)=>{
  fixture("left",path,archive);
  usePanes.setState(s=>({left:{...s.left,tabs:[{...s.left.tabs[0],entries:[row("other",{is_dir:true}),row("clicked",{is_dir:true})],cursor:0,selected:new Set(["other"])}]}}));
  const targetRef=createRef<number|null>();targetRef.current=1;
  render(<FileContextMenu side="left" targetRef={targetRef}><div>target</div></FileContextMenu>);
  const user=userEvent.setup();await user.pointer({target:screen.getByText("target"),keys:"[MouseRight]"});
  await user.click(screen.getByRole("menuitem",{name:translate("ctx.openOpposite")}));
  await waitFor(()=>expect(panelOf(usePanes.getState(),"right").path).toBe(`${path}/clicked`));
  expect(panelOf(usePanes.getState(),"left").path).toBe(path);
  expect(panelOf(usePanes.getState(),"right").archive).toBe(archive);
});
