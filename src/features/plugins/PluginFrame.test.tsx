import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { PluginFrame } from "./PluginFrame";

describe("plugin frame bridge", () => {
  it("forwards only its opaque-origin frame and rejects foreign senders", async () => {
    const harness = useTauri({commands: {
      plugin_request: () => "pong", plugin_poll: () => [], plugin_close: () => {},
    }});
    const {unmount} = render(<PluginFrame id="test" title="Test plugin" />);
    const frame = screen.getByTitle("Test plugin") as HTMLIFrameElement;
    expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
    const data = {channel:"hortcmd:request", id:1, request:{type:"ping"}};
    fireEvent(window, new MessageEvent("message", {source:window, origin:"null", data}));
    fireEvent(window, new MessageEvent("message", {source:frame.contentWindow, origin:"https://example.com", data}));
    expect(harness.calls.filter(call => call.cmd === "plugin_request")).toHaveLength(0);
    await act(async () => { fireEvent(window, new MessageEvent("message", {source:frame.contentWindow, origin:"null", data})); });
    expect(harness.calls.filter(call => call.cmd === "plugin_request")).toHaveLength(1);
    unmount();
    await act(async () => { fireEvent(window, new MessageEvent("message", {source:frame.contentWindow, origin:"null", data})); });
    expect(harness.calls.filter(call => call.cmd === "plugin_request")).toHaveLength(1);
  });
  it("falls back and closes the instance after a broker failure", async () => {
    const harness = useTauri({commands: {plugin_poll: () => {throw new Error("revoked");}, plugin_close: () => {}}});
    const fail = vi.fn();
    render(<PluginFrame id="revoked" title="Revoked" onFailure={fail} />);
    await waitFor(() => expect(fail).toHaveBeenCalledOnce());
    expect(harness.calls.some(c => c.cmd === "plugin_close")).toBe(true);
    expect(screen.queryByTitle("Revoked")).not.toBeInTheDocument();
  });
});
