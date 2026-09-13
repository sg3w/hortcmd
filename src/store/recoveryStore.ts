import { create } from "zustand";
import type { RecoveryEntry } from "@/ipc/bindings/RecoveryEntry";
import { listRecovery } from "@/ipc/client";
import { describeErrorFull } from "./errorText";
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
  setOpen: open => set({ open }),
  refresh: async (startup = false) => {
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
export function openRecovery(): void {
  useRecovery.getState().setOpen(true);
  void useRecovery.getState().refresh();
}
