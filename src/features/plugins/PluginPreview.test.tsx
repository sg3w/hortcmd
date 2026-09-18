import { render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
it("retains the built-in preview when matching fails", async () => {
  const harness = useTauri({commands:{plugin_workspace:() => {}, plugin_preview:() => {throw new Error("unavailable");}}});
  vi.resetModules(); const {default: PluginPreview} = await import("./PluginPreview");
  render(<PluginPreview path="/file.php">Built-in source</PluginPreview>);
  expect(screen.getByText("Built-in source")).toBeInTheDocument();
  await waitFor(() => expect(harness.calls.some(c=>c.cmd === "plugin_preview")).toBe(true));
  expect(screen.getByText("Built-in source")).toBeInTheDocument();
});
it("closes a late preview result after unmount", async () => {
  let resolve!: (value:unknown)=>void;
  const harness=useTauri({commands:{plugin_workspace:()=>{},plugin_preview:()=>new Promise(r=>{resolve=r;}),plugin_close:()=>{}}});
  vi.resetModules(); const {default: PluginPreview}=await import("./PluginPreview");
  const {unmount}=render(<PluginPreview path="/file.php">Built-in source</PluginPreview>);
  await waitFor(()=>expect(resolve).toBeDefined()); unmount(); resolve({id:"late"});
  await waitFor(()=>expect(harness.calls.find(c=>c.cmd === "plugin_close")?.args.id).toBe("late"));
});
