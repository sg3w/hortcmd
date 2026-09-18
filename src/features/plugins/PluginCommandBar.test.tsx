import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useDemo, useTauri } from "@/test/tauri";
import type { PluginRecord } from "@/ipc/bindings/PluginRecord";
async function show() {
  vi.resetModules();
  const {default: PluginCommandBar} = await import("./PluginCommandBar");
  const {usePlugins} = await import("./pluginsStore");
  return {...render(<PluginCommandBar />), usePlugins};
}
const plugin = (active = true): PluginRecord => ({id:"org.test.tools", active, granted:active ? ["ui:commandbar", "ui:window"] : [], error:null, manifest:{
  id:"org.test.tools", name:"Tools", version:"1.0.0", description:"Test", author:"Test", apiVersion:1,
  runtime:"js", entry:"index.html", icon:"🌈", permissions:["ui:commandbar", "ui:window"],
  contributes:{commandbar:[{id:"inspect", title:"Inspect", icon:"🌈", action:{type:"openWindow"}}], preview:[], tabs:[]},
}});
describe("plugin command bar", () => {
  it("is absent in the browser demo and for inactive plugins", async () => {
    useDemo();
    const {unmount} = await show();
    expect(screen.queryByRole("button")).not.toBeInTheDocument(); unmount();
    useTauri({commands:{plugin_workspace:() => {}, list_plugins: () => [plugin(false)]}});
    const {usePlugins} = await show();
    await waitFor(() => expect(usePlugins.getState().records).toHaveLength(1));
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
  it("shows the colorful icon and sends only the declared contribution id", async () => {
    const harness = useTauri({commands:{plugin_workspace:() => {}, list_plugins:() => [plugin()], plugin_launch:() => ({})}});
    const {usePlugins} = await show();
    fireEvent.click(await screen.findByRole("button", {name:"Inspect"}));
    await waitFor(() => expect(harness.calls.some(c => c.cmd === "plugin_launch")).toBe(true));
    expect(harness.calls.find(c => c.cmd === "plugin_launch")?.args).toEqual({plugin:"org.test.tools", button:"inspect"});
    expect(screen.getByText("🌈")).toBeInTheDocument();
    expect(screen.getByRole("group", {name:"Tools"})).toBeInTheDocument();
    act(() => usePlugins.setState({records:[plugin(false)]}));
    await waitFor(() => expect(screen.queryByRole("button")).not.toBeInTheDocument());
  });
  it("displays backend launch errors without discarding the commander UI", async () => {
    useTauri({commands:{plugin_workspace:() => {}, list_plugins:() => [plugin()], plugin_launch:() => {throw new Error("Permission denied");}}});
    await show();
    fireEvent.click(await screen.findByRole("button", {name:"Inspect"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("Permission denied");
  });
});
