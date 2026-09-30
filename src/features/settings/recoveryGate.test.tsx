// ============================================================
// The beta gate around transfer recovery (TICKET-396): what the backend
// is told, and what the feature offers while the switch is off.
// ============================================================

import { render, waitFor } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import { useTauri } from "@/test/tauri";
import { COMMANDS } from "@/features/commander/commands";
import { useSettings } from "@/store/settingsStore";
import { openRecovery, useRecovery } from "@/store/recoveryStore";
import { useRecoveryGate } from "./useRecoveryGate";

function Gate() { useRecoveryGate(); return null; }

beforeEach(() => useRecovery.setState({ open: false, entries: [] }));

it("pushes the switch to the backend and looks at the journal only once it is on", async () => {
  const runtime = useTauri({ commands: { recovery_set_enabled: () => {}, recovery_list: () => [] } });
  expect(useSettings.getState().recoveryEnabled).toBe(false);
  const view = render(<Gate />);
  await waitFor(() =>
    expect(runtime.calls).toContainEqual({ cmd: "recovery_set_enabled", args: { enabled: false } }),
  );
  // Off, the journal is never read: the backend answers empty by design,
  // and asking would only create the storage the feature is not using.
  expect(runtime.calls.some(c => c.cmd === "recovery_list")).toBe(false);

  useSettings.setState({ recoveryEnabled: true });
  view.rerender(<Gate />);
  await waitFor(() =>
    expect(runtime.calls).toContainEqual({ cmd: "recovery_set_enabled", args: { enabled: true } }),
  );
  await waitFor(() => expect(runtime.calls.some(c => c.cmd === "recovery_list")).toBe(true));
});

it("closes an open list when the switch goes off", async () => {
  const runtime = useTauri({ commands: { recovery_set_enabled: () => {}, recovery_list: () => [] } });
  useSettings.setState({ recoveryEnabled: true });
  const view = render(<Gate />);
  await waitFor(() =>
    expect(runtime.calls).toContainEqual({ cmd: "recovery_set_enabled", args: { enabled: true } }),
  );
  useRecovery.setState({ open: true });
  useSettings.setState({ recoveryEnabled: false });
  view.rerender(<Gate />);
  await waitFor(() => expect(useRecovery.getState().open).toBe(false));
});

// While the switch is off the feature has no entry point at all — not the
// menu, not `openRecovery`, which is what everything else goes through.
it("offers no entry point while transfer recovery is switched off", async () => {
  const runtime = useTauri({ commands: { recovery_list: () => [] } });
  expect(useSettings.getState().recoveryEnabled).toBe(false);
  openRecovery();
  expect(useRecovery.getState().open).toBe(false);
  expect(runtime.calls).toEqual([]);
  expect(COMMANDS["cmd.recovery"].state?.()).toEqual({ hidden: true });

  useSettings.setState({ recoveryEnabled: true });
  expect(COMMANDS["cmd.recovery"].state?.()).toEqual({ hidden: false });
  openRecovery();
  expect(useRecovery.getState().open).toBe(true);
  await waitFor(() => expect(runtime.calls.some(c => c.cmd === "recovery_list")).toBe(true));
});
