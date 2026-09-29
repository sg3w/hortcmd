import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useDemo, useTauri } from "@/test/tauri";
import { openUndo, useUndo } from "@/store/undoStore";
import { useSettings } from "@/store/settingsStore";
import { usePanes } from "@/store/panesStore";
import { useKeyboard } from "@/features/commander/useKeyboard";
import { COMMANDS } from "@/features/commander/commands";
import { UndoDialog } from "./UndoDialog";
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
  useSettings.setState({ undoEnabled: true });
  render(<Keyboard />); const user = userEvent.setup();
  const input = screen.getByRole("textbox"); await user.click(input);
  await user.keyboard("{Control>}{Alt>}z{/Alt}{/Control}");
  expect(useUndo.getState().open).toBe(false);
  input.blur(); await user.keyboard("{Control>}{Alt>}z{/Alt}{/Control}");
  expect(useUndo.getState().open).toBe(true);
});

// The beta gate (TICKET-391): off is the default, and while it is off the
// feature has no entry point at all — not the shortcut, not the menu, not
// `openUndo` itself, which is what everything else goes through.
it("offers no entry point while file undo is switched off", async () => {
  useTauri({ commands: {} }); useUndo.setState({ open: false });
  expect(useSettings.getState().undoEnabled).toBe(false);
  render(<Keyboard />); const user = userEvent.setup();
  screen.getByRole("textbox").blur();
  await user.keyboard("{Control>}{Alt>}z{/Alt}{/Control}");
  expect(useUndo.getState().open).toBe(false);
  openUndo();
  expect(useUndo.getState().open).toBe(false);
  expect(COMMANDS["file.undo"].state?.()).toEqual({ hidden: true });
  useSettings.setState({ undoEnabled: true });
  expect(COMMANDS["file.undo"].state?.()).toEqual({ hidden: false });
  openUndo();
  expect(useUndo.getState().open).toBe(true);
});

it("uses native IPC from a statically imported component after switching modes", async () => {
  useDemo();
  const runtime = useTauri({ commands: { undo_preview: () => [{
    id: "native-static", group: null, kind: "rename", path: "/native-new",
    target: "/native-old", status: "ready", retained: null,
  }] } });
  useUndo.setState({ open: true });
  render(<UndoDialog />);
  await screen.findByText("/native-new → /native-old");
  expect(runtime.calls).toContainEqual({ cmd: "undo_preview", args: {} });
});
