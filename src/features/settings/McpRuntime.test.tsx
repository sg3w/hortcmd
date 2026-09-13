import { expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { listenerCount, useTauri } from "@/test/tauri";

async function setup() {
  const harness = useTauri({commands: {mcp_status: () => ({running:false}), mcp_answer: () => undefined}});
  vi.resetModules();
  const { McpRuntime } = await import("./McpRuntime");
  const view = render(<McpRuntime />);
  await waitFor(() => expect(listenerCount("mcp-confirm-closed")).toBe(1));
  return {harness, view};
}

it("removes revoked questions and unsubscribes both event listeners", async () => {
  const {harness, view} = await setup();
  act(() => harness.emit("mcp-confirm", {id:"one",client:"client",tool:"copy",summary:"copy requested files"}));
  expect(await screen.findByText("copy requested files")).toBeInTheDocument();
  act(() => harness.emit("mcp-confirm-closed", "one"));
  await waitFor(() => expect(screen.queryByText("copy requested files")).not.toBeInTheDocument());
  expect(harness.calls.some(c => c.cmd === "mcp_answer")).toBe(false);
  view.unmount();
  await waitFor(() => expect(listenerCount("mcp-confirm")).toBe(0));
  expect(listenerCount("mcp-confirm-closed")).toBe(0);
});

it("sends the question identity and resets remembered approval for the next question", async () => {
  const {harness} = await setup();
  act(() => harness.emit("mcp-confirm", {id:"one",client:"client",tool:"copy",summary:"first request"}));
  fireEvent.click(await screen.findByLabelText("Dieses Werkzeug für diese Sitzung erlauben"));
  fireEvent.click(screen.getByRole("button", {name:"Erlauben"}));
  await waitFor(() => expect(harness.calls.find(c => c.cmd === "mcp_answer")?.args).toEqual({id:"one",approved:true,remember:true}));
  await waitFor(() => expect(screen.queryByText("first request")).not.toBeInTheDocument());
  act(() => harness.emit("mcp-confirm", {id:"two",client:"client",tool:"delete",summary:"second request"}));
  expect(await screen.findByLabelText("Dieses Werkzeug für diese Sitzung erlauben")).not.toBeChecked();
});
