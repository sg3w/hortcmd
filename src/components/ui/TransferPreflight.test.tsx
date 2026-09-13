import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { preflightFixture } from "@/test/preflight";
import { useOps } from "@/store/opsStore";
import { useSettings } from "@/store/settingsStore";
import { OperationDialogs } from "./OperationDialogs";
import type { PreflightRequest } from "@/ipc/domains/preflight";
vi.mock("@/ipc/invoke", async original => ({ ...await original<typeof import("@/ipc/invoke")>(), hasTauri: true }));
const request: PreflightRequest = { kind: "copy", sources: ["/src/a"], destination: "/dst", settings: { safe: true, verify: false, limit: 0, bufKb: 256, threads: 2 } };
it.each(["de", "en"] as const)("blocks known capacity failures before dispatch in %s", async language => {
  const plan = preflightFixture(request); plan.issues = [{ severity: "blocking", code: "noSpace", item: null }];
  useTauri({ commands: { transfer_preflight: () => plan, cancel_preflight: () => {} } });
  useSettings.setState({ language }); const onConfirm = vi.fn();
  useOps.getState().requestConfirm({ title: "Copy", message: "/src/a → /dst", preflight: request, onConfirm });
  render(<OperationDialogs />);
  await screen.findByText(language === "de" ? /Der bekannte freie Platz reicht/ : /Known available space is insufficient/);
  const button = screen.getByRole("button", { name: "OK" });
  expect(button).toBeDisabled(); await userEvent.setup().click(button); expect(onConfirm).not.toHaveBeenCalled();
});
it("passes the immutable reviewed settings and cancels an abandoned traversal", async () => {
  let resolve!: (value: ReturnType<typeof preflightFixture>) => void;
  const runtime = useTauri({ commands: { transfer_preflight: () => new Promise(r => { resolve = r; }), cancel_preflight: () => {} } });
  useSettings.setState({ language: "en" }); const onConfirm = vi.fn();
  useOps.getState().requestConfirm({ title: "Copy", message: "Files", preflight: request, onConfirm });
  render(<OperationDialogs />);
  await waitFor(() => expect(runtime.calls.some(c => c.cmd === "transfer_preflight")).toBe(true));
  const user = userEvent.setup(); await user.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(runtime.calls.some(c => c.cmd === "cancel_preflight")).toBe(true));
  resolve(preflightFixture(request));
  expect(onConfirm).not.toHaveBeenCalled();
});

it("approval hands over the reviewed plan instead of later settings", async () => {
  const plan = preflightFixture(request);
  useTauri({ commands: { transfer_preflight: () => plan, cancel_preflight: () => {} } });
  useSettings.setState({ language: "en" }); const onConfirm = vi.fn();
  useOps.getState().requestConfirm({ title: "Copy", message: "Files", preflight: request, onConfirm });
  render(<OperationDialogs />);
  const button = screen.getByRole("button", { name: "OK" });
  await waitFor(() => expect(button).toBeEnabled());
  useSettings.setState({ safeTransfers: false, verifyCopies: true });
  await userEvent.setup().click(button);
  expect(onConfirm).toHaveBeenCalledWith(expect.objectContaining({ digest: "reviewed-plan", settings: expect.objectContaining({ safe: true, verify: false }) }));
});
