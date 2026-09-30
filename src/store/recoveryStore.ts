import { create } from "zustand";
import type { RecoveryEntry } from "@/ipc/bindings/RecoveryEntry";
import { listRecovery } from "@/ipc/client";
import { describeErrorFull } from "./errorText";
import { useSettings } from "./settingsStore";
import { useTransfers } from "./transfersStore";

interface RecoveryStore {
  open: boolean;
  loading: boolean;
  entries: RecoveryEntry[];
  error: string | null;
  setOpen: (open: boolean) => void;
  refresh: (startup?: boolean) => Promise<void>;
}
let generation = 0;
export const useRecovery = create<RecoveryStore>((set) => ({
  open: false, loading: false, entries: [], error: null,
  setOpen: open => set({ open: open && recoveryAvailable() }),
  refresh: async (startup = false) => {
    // Beta and off by default (TICKET-396): nothing is journaled while the
    // switch is off, so there is nothing to ask about either — and the
    // startup dialog must not appear for a feature nobody turned on.
    if (!recoveryAvailable()) {
      set({ entries: [], loading: false, error: null, open: false });
      return;
    }
    const request = ++generation;
    set({ loading: true, error: null });
    try {
      const entries = await listRecovery();
      if (generation !== request) return;
      // A reloaded webview can reattach to native work without restarting it.
      for (const entry of entries) {
        if (entry.state === "completed" && useTransfers.getState().transfers.some(t => t.id === entry.id && !t.done)) {
          useTransfers.getState().finish({ id: entry.id, op: entry.kind, ok: entry.committed, mode: "", cancelled: false, errors: [] });
        }
        if (entry.state === "running" && !useTransfers.getState().transfers.some(t => t.id === entry.id)) {
          useTransfers.getState().start(entry.kind, entry.id);
          useTransfers.getState().minimize(entry.id, true);
        }
      }
      set({ entries, loading: false, ...(startup && entries.some(e => e.state !== "running") ? { open: true } : {}) });
    } catch (error) {
      if (generation !== request) return;
      set({ loading: false, error: describeErrorFull(error), ...(startup ? { open: true } : {}) });
    }
  },
}));
/** Whether transfer recovery is switched on (read, never subscribed — see TICKET-354). */
export function recoveryAvailable(): boolean {
  return useSettings.getState().recoveryEnabled;
}

export function openRecovery(): void {
  if (!recoveryAvailable()) return;
  useRecovery.getState().setOpen(true);
  void useRecovery.getState().refresh();
}
