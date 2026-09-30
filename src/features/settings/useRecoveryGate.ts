// ============================================================
// Keeps the backend's copy of the transfer-recovery switch in step with
// the setting (TICKET-396).
//
// The same arrangement as `useUndoGate`: the backend cannot read
// `localStorage`, and the journal is written from the transfer engine,
// extraction and packing — none of which carry the settings — so the
// switch is pushed once at startup and again whenever it changes. The
// backend defaults to off, which is what makes the gap before the first
// push harmless.
// ============================================================

import { useEffect, useRef } from "react";
import { setRecoveryEnabled } from "@/ipc/client";
import { useSettings } from "@/store/settingsStore";
import { useRecovery } from "@/store/recoveryStore";

export function useRecoveryGate(): void {
  const enabled = useSettings((s) => s.recoveryEnabled);
  // Only the first push is a startup: turning the switch on from the
  // settings reloads the list, but does not throw a dialog over the page
  // the user is standing on.
  const startup = useRef(true);
  useEffect(() => {
    const first = startup.current;
    startup.current = false;
    void setRecoveryEnabled(enabled)
      .then(() => {
        // The list is only worth asking for once the backend knows the
        // switch is on — before that it answers empty by design.
        if (enabled) void useRecovery.getState().refresh(first);
      })
      .catch(() => {
        /* The backend keeps its previous value; off is its default. */
      });
    // Switching the feature off while its list is open closes it — the
    // entries stay journaled, they are simply no longer offered.
    if (!enabled) useRecovery.getState().setOpen(false);
  }, [enabled]);
}
