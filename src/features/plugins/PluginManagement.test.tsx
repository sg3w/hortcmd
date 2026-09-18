import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import type { PluginRecord } from "@/ipc/bindings/PluginRecord";
const plugin=():PluginRecord=>({id:"org.test.native",fingerprint:"verified-package",active:false,granted:[],error:null,manifest:{id:"org.test.native",name:"Native tools",version:"1.0.0",description:"Tools",author:"Example",apiVersion:1,runtime:"native",entry:{macos:"tools.dylib",windows:null,linux:null},icon:"🌈",permissions:["read:fs","notify"],contributes:{commandbar:[],tabs:[],preview:[]}}});
async function show(){vi.resetModules();const {PluginManagement}=await import("./PluginManagement");return render(<PluginManagement/>);}
it("requires explicit approval, presents native privileges, and refreshes after revocation",async()=>{
  const record=plugin();const harness=useTauri({commands:{list_plugins:()=>[record],plugin_activate:()=>{record.active=true;record.granted=["read:fs","notify"];},plugin_deactivate:()=>{record.active=false;record.granted=[];}}});
  await show();fireEvent.click(await screen.findByRole("button",{name:"Activate"}));
  expect(screen.getByRole("note")).toHaveTextContent("not a security sandbox");expect(screen.getByText("read:fs")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"Cancel"}));expect(harness.calls.some(c=>c.cmd==="plugin_activate")).toBe(false);
  fireEvent.click(screen.getByRole("button",{name:"Activate"}));fireEvent.click(screen.getByRole("button",{name:"Approve and activate"}));
  fireEvent.click(await screen.findByRole("button",{name:"Deactivate"}));
  await waitFor(()=>expect(harness.calls.filter(c=>c.cmd==="plugin_activate")).toHaveLength(1));
  expect(harness.calls.find(c=>c.cmd==="plugin_activate")?.args).toEqual({plugin:record.id,fingerprint:"verified-package"});
  await waitFor(()=>expect(screen.getByRole("button",{name:"Activate"})).toBeEnabled());
});
it("reports installation failures and confirms uninstall",async()=>{
  const record=plugin();let present=true;
  const harness=useTauri({commands:{list_plugins:()=>present?[record]:[],plugin_install:()=>{throw new Error("Unsafe archive path");},plugin_deactivate:()=>{present=false;}}});
  await show();fireEvent.change(screen.getByRole("textbox"),{target:{value:"/tmp/plugin.zip"}});fireEvent.click(screen.getByRole("button",{name:"Install"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("Unsafe archive path");
  await waitFor(()=>expect(screen.getByRole("button",{name:"Uninstall"})).toBeEnabled());
  fireEvent.click(screen.getByRole("button",{name:"Uninstall"}));
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button",{name:"Uninstall"}));
  await waitFor(()=>expect(screen.getByText("No plugins installed.")).toBeInTheDocument());
  expect(harness.calls.find(c=>c.cmd==="plugin_deactivate")?.args).toEqual({plugin:record.id,remove:true});
});
