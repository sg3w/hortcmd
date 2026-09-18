import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { useDemo } from "@/test/tauri";
const html=readFileSync("plugins/path-tools/index.html","utf8");
const script=readFileSync("plugins/path-tools/app.js","utf8");
const context={path:"/data/<source>",folderPath:"/data",filePath:"/data/<source>",selection:["/data/<source>","/data/second"],side:"left",copyLimit:128};
async function mount(overrides={}) {
  useDemo();document.body.innerHTML=new DOMParser().parseFromString(html,"text/html").body.innerHTML;
  const listeners=new Set();
  const request=vi.fn(async message=>{
    if(message.type==="context:get")return context;
    if(message.type==="op:copy")return {operationId:"operation-1",status:"awaiting-confirmation"};
    return null;
  });
  const api={request,onEvent:listener=>{listeners.add(listener);return()=>listeners.delete(listener);},...overrides};
  vi.stubGlobal("hortcmd",api);window.eval(script);
  await waitFor(()=>expect(document.getElementById("path").textContent).toBe(context.path));
  return {api,listeners};
}
afterEach(()=>document.body.replaceChildren());
it("renders source context as text and submits selection through the broker without changing its limit",async()=>{
  const {api}=await mount();expect(document.querySelector("source")).toBeNull();expect(screen.getByText("128 KiB/s")).toBeVisible();
  fireEvent.change(screen.getByLabelText("Destination folder"),{target:{value:"/destination"}});
  fireEvent.submit(document.getElementById("copy-form"));
  await waitFor(()=>expect(screen.getByRole("status")).toHaveTextContent("review it in the commander"));
  expect(api.request).toHaveBeenCalledWith({type:"op:copy",sources:context.selection,destination:"/destination"});
  expect(api.request.mock.calls.some(([request])=>request.type==="op:limit")).toBe(false);
});
it("copies the displayed path only on a user gesture and reports clipboard refusal",async()=>{
  await mount();document.execCommand=vi.fn(()=>true);
  fireEvent.click(screen.getByRole("button",{name:"Copy path"}));
  expect(document.execCommand).toHaveBeenCalledWith("copy");expect(screen.getByRole("status")).toHaveTextContent("Path copied.");
  document.execCommand=vi.fn(()=>false);fireEvent.click(screen.getByRole("button",{name:"Copy path"}));
  await waitFor(()=>expect(screen.getByRole("status")).toHaveTextContent("Clipboard access is unavailable"));
  delete document.execCommand;
});
it("updates context on request and removes event subscriptions on closure",async()=>{
  const {api,listeners}=await mount();api.request.mockResolvedValue({...context,path:"/new"});
  fireEvent.click(screen.getByRole("button",{name:"Refresh context"}));
  await waitFor(()=>expect(document.getElementById("path")).toHaveTextContent("/new"));
  for(const listener of listeners)listener({type:"view:close"});expect(listeners.size).toBe(0);
});
it("prevents duplicate copy submission and shows a denied broker request",async()=>{
  const {api}=await mount();let reject;api.request.mockImplementation(()=>new Promise((_,no)=>{reject=no;}));
  fireEvent.change(screen.getByLabelText("Destination folder"),{target:{value:"/destination"}});
  const before=api.request.mock.calls.length;fireEvent.submit(document.getElementById("copy-form"));fireEvent.submit(document.getElementById("copy-form"));
  expect(api.request.mock.calls.length-before).toBe(1);reject(new Error("Permission denied"));
  await waitFor(()=>expect(screen.getByRole("status")).toHaveTextContent("Permission denied"));
  expect(screen.getByRole("button",{name:"Copy selection to destination"})).toBeEnabled();
});
