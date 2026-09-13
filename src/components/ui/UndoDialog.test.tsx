import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { useUndo } from "@/store/undoStore";
import { useSettings } from "@/store/settingsStore";
import { usePanes } from "@/store/panesStore";
import { useKeyboard } from "@/features/commander/useKeyboard";
import { UndoDialog } from "./UndoDialog";
vi.mock("@/ipc/invoke", async original => ({ ...await original<typeof import("@/ipc/invoke")>(), hasTauri: true }));
it.each(["de", "en"] as const)("reviews exact paths and disables changed entries before undo in %s", async language => {
  const entry = { id: "a", group: null, kind: "rename", path: "/new", target: "/old", status: "ready", retained: null };
  const runtime = useTauri({ commands: { undo_preview: () => [entry, { ...entry, id: "b", path: "/changed", status: "changed" }], undo_execute: () => [{ ...entry, status: "restored" }] } });
  useSettings.setState({ language }); useUndo.setState({ open: true });
  vi.spyOn(usePanes.getState(), "refresh").mockResolvedValue();
  render(<UndoDialog />);
  await screen.findByText("/new → /old");
  const buttons = screen.getAllByRole("button", { name: language === "de" ? "Diese Aktion rückgängig machen" : "Undo this operation" });
  expect(buttons[0]).toBeEnabled(); expect(buttons[1]).toBeDisabled();
  expect(runtime.calls.some(c => c.cmd === "undo_execute")).toBe(false);
  await userEvent.setup().click(buttons[0]);
  await waitFor(() => expect(runtime.calls).toContainEqual({ cmd: "undo_execute", args: { id: "a" } }));
});
function Keyboard() { useKeyboard(); return <input aria-label="text" />; }
it("leaves text editing undo and its modifier variants in the focused field", async () => {
  useTauri({ commands: {} }); useUndo.setState({ open: false });
  render(<Keyboard />); const user = userEvent.setup();
  const input = screen.getByRole("textbox"); await user.click(input);
  await user.keyboard("{Control>}{Alt>}z{/Alt}{/Control}");
  expect(useUndo.getState().open).toBe(false);
  input.blur(); await user.keyboard("{Control>}{Alt>}z{/Alt}{/Control}");
  expect(useUndo.getState().open).toBe(true);
});
