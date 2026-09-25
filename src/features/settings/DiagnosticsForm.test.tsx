import { expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useDemo, useTauri } from "@/test/tauri";
const status = { version: "0.1.0", platform: "test", architecture: "test", build: "development", level: "info", location: "/private/logs", available: true, recent: [], providers: ["local"], categories: ["structured lifecycle events"], configuration: ["no upload"] };
async function mount() { vi.resetModules(); const { DiagnosticsForm } = await import("./DiagnosticsForm"); render(<DiagnosticsForm />); }
it("previews categories before an explicit export and keeps identifiers excluded by default", async () => {
  const harness = useTauri({ commands: { diagnostics_status: () => status, diagnostics_export: () => {}, diagnostics_clear: () => {}, diagnostics_level: () => {} } });
  await mount(); await screen.findByText("structured lifecycle events");
  expect(harness.calls.some(c => c.cmd === "diagnostics_export")).toBe(false);
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "/tmp/support.json" } });
  fireEvent.click(screen.getByRole("button", { name: "Supportpaket exportieren" }));
  await waitFor(() => expect(harness.calls.find(c => c.cmd === "diagnostics_export")?.args).toEqual({ destination: "/tmp/support.json", includeLocation: false }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Diagnosedaten löschen" })).not.toBeDisabled());
  fireEvent.click(screen.getByRole("button", { name: "Diagnosedaten löschen" }));
  await waitFor(() => expect(harness.calls.some(c => c.cmd === "diagnostics_clear")).toBe(true));
});
it("does not offer a native export in browser demo", async () => { useDemo(); await mount(); expect(screen.queryByRole("textbox")).toBeNull(); });
