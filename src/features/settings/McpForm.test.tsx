import { expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useDemo, useTauri } from "@/test/tauri";
import type { McpConfig } from "@/ipc/bindings/McpConfig";
async function showForm() {
  vi.resetModules();
  const { McpForm } = await import("./McpForm");
  render(<McpForm />);
}

const config: McpConfig = {
  enabled: false, port: 8712, allowRead: true, allowWrite: false,
  allowDelete: false, allowPermanentDelete: false, confirmWrites: true,
  roots: [], rootsFollowPanes: false, rootsFollowRemotes: false, maxReadBytes: 2097152,
};

it("shows the native-only explanation in the browser demo", async () => {
  useDemo();
  await showForm();
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
});

it("saves independent read and connected-remote root choices through typed IPC", async () => {
  const harness = useTauri({ commands: {
    mcp_status: () => ({config, endpoint: null, running: false, boundPort: null, hasToken: false, clients: 0}),
    mcp_audit: () => [],
    mcp_set_config: args => args.config,
  }});
  await showForm();
  fireEvent.click(await screen.findByLabelText("Lesen erlauben"));
  fireEvent.click(screen.getByLabelText("Aktuell verbundene Remote-Orte freigeben"));
  fireEvent.click(screen.getByRole("button", {name: "Übernehmen"}));
  await waitFor(() => expect(harness.calls.find(call => call.cmd === "mcp_set_config")?.args.config)
    .toEqual({...config, allowRead: false, rootsFollowRemotes: true}));
  expect(harness.calls.some(call => call.cmd === "mcp_token")).toBe(false);
});

it("reports a failed settings load without enabling the server", async () => {
  const harness = useTauri({ commands: {
    mcp_status: () => { throw new Error("unavailable"); }, mcp_audit: () => [],
  }});
  await showForm();
  await waitFor(() => expect(screen.getByRole("status").textContent).not.toMatch(/laden/i));
  expect(harness.calls.some(call => call.cmd === "mcp_set_config")).toBe(false);
});
