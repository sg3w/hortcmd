import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { useRecovery } from "@/store/recoveryStore";
import { useSettings } from "@/store/settingsStore";
import type { RecoveryEntry } from "@/ipc/bindings/RecoveryEntry";
import { RecoveryDialog } from "./RecoveryDialog";

vi.mock("@/ipc/invoke", async original => ({ ...await original<typeof import("@/ipc/invoke")>(), hasTauri: () => true }));
const entry: RecoveryEntry = { id: "interrupted", kind: "move", state: "cleanupRequired", order: 1,
  sources: [{ kind: "local", path: "/source/a.txt" }], destination: { kind: "local", path: "/destination" }, committed: 0, total: 1, reason: null };

it.each(["de", "en"] as const)("requires explicit cleanup confirmation and preserves the source in %s", async language => {
  const runtime = useTauri({ commands: { recovery_list: () => [], recovery_cleanup: () => {} } });
  useSettings.setState({ language });
  useRecovery.setState({ open: true, entries: [entry] });
  render(<RecoveryDialog />);
  const user = userEvent.setup();
  expect(runtime.calls).toEqual([]);
  await user.click(screen.getByRole("button", { name: language === "de" ? "Temporäre Daten bereinigen" : "Clean temporary data" }));
  expect(runtime.calls).toEqual([]);
  await user.click(screen.getByRole("button", { name: language === "de" ? "Abbrechen" : "Cancel" }));
  expect(runtime.calls).toEqual([]);
  await user.click(screen.getByRole("button", { name: language === "de" ? "Temporäre Daten bereinigen" : "Clean temporary data" }));
  await user.click(screen.getByRole("button", { name: language === "de" ? "Bestätigen" : "Confirm" }));
  await waitFor(() => expect(runtime.calls).toContainEqual({ cmd: "recovery_cleanup", args: { id: entry.id } }));
  expect(runtime.calls.some(c => c.cmd === "delete_entries" || c.cmd === "move_entries")).toBe(false);
});

it("does not offer resume or retry for unverifiable or disconnected operations", () => {
  useTauri({ commands: {} });
  useSettings.setState({ language: "en" });
  useRecovery.setState({ open: true, entries: [
    { ...entry, state: "unrecoverable" }, { ...entry, id: "remote", state: "unavailable" },
  ] });
  render(<RecoveryDialog />);
  expect(screen.queryByRole("button", { name: "Resume" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Retry from start" })).toBeNull();
});

it("shows persisted queue order and starts no operation merely by opening", () => {
  const runtime = useTauri({ commands: {} });
  useSettings.setState({ language: "en" });
  useRecovery.setState({ open: true, entries: [{ ...entry, id: "b", state: "queued", destination: { kind: "local", path: "/first" } }, { ...entry, id: "a", state: "queued", order: 2, destination: { kind: "local", path: "/second" } }] });
  render(<RecoveryDialog />);
  expect(screen.getAllByRole("heading", { level: 3 })).toHaveLength(2);
  expect(screen.getByText("/first").compareDocumentPosition(screen.getByText("/second")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(runtime.calls).toEqual([]);
});
