// ============================================================
// The two halves of the beta gate that are not the switch itself
// (TICKET-391): what the backend is told, and what the operation
// dialogs say while file undo is off.
// ============================================================

import { render, waitFor } from "@testing-library/react";
import { expect, it } from "vitest";
import { useTauri } from "@/test/tauri";
import { useSettings } from "@/store/settingsStore";
import { useUndo } from "@/store/undoStore";
import { useUndoClassification } from "@/components/ui/useUndoClassification";
import { useUndoGate } from "./useUndoGate";

const REQUEST = { kind: "copy", sources: ["/a"], target: "/b", directory: false };

function Gate() { useUndoGate(); return null; }

it("pushes the switch to the backend and closes an open history when it goes off", async () => {
  const runtime = useTauri({ commands: { undo_set_enabled: () => {} } });
  useSettings.setState({ undoEnabled: true });
  const view = render(<Gate />);
  await waitFor(() =>
    expect(runtime.calls).toContainEqual({ cmd: "undo_set_enabled", args: { enabled: true } }),
  );

  useUndo.setState({ open: true });
  useSettings.setState({ undoEnabled: false });
  view.rerender(<Gate />);
  await waitFor(() =>
    expect(runtime.calls).toContainEqual({ cmd: "undo_set_enabled", args: { enabled: false } }),
  );
  expect(useUndo.getState().open).toBe(false);
});

function Classified() {
  const value = useUndoClassification(REQUEST);
  return <span data-testid="value">{value === undefined ? "none" : value ?? "checking"}</span>;
}

it("asks the backend nothing and says nothing while file undo is off", async () => {
  const runtime = useTauri({ commands: { undo_classify: () => "conditional" } });
  expect(useSettings.getState().undoEnabled).toBe(false);
  const view = render(<Classified />);
  // No sentence at all — not "cannot be undone", which would describe a
  // limit of the feature rather than the fact that it is not running.
  expect(view.getByTestId("value").textContent).toBe("none");
  expect(runtime.calls.some(c => c.cmd === "undo_classify")).toBe(false);

  useSettings.setState({ undoEnabled: true });
  view.rerender(<Classified />);
  await waitFor(() => expect(view.getByTestId("value").textContent).toBe("conditional"));
});
