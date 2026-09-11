// ============================================================
// Remote connections (SFTP, ticket 021/022).
//
// Holds the saved connection profiles and their live status, plus the
// manager-dialog state. Secrets never live here — they are passed to
// the backend on save and stored in the OS keychain.
// ============================================================

import { create } from "zustand";
import type { ConnectionStatus, Profile } from "@/ipc/bindings";
import { usePanes } from "@/store/panesStore";
import {
  netConnect,
  netDeleteProfile,
  netDisconnect,
  netListConnections,
  netListProfiles,
  netSaveProfile,
} from "@/ipc/client";

interface NetState {
  profiles: Profile[];
  connections: ConnectionStatus[];
  managerOpen: boolean;
  /** Profile currently being edited in the form, or null for the list view. */
  editing: Profile | null;
  error: string | null;

  refresh: () => Promise<void>;
  /** `keepError` shows the manager *with* the last error — the Netz menu
   *  connects without the dialog open and needs a place to report it. */
  openManager: (keepError?: boolean) => void;
  closeManager: () => void;
  startEdit: (profile: Profile | null) => void;
  cancelEdit: () => void;
  save: (profile: Profile, secret: string | null) => Promise<void>;
  remove: (id: string) => Promise<void>;
  /** Connects and returns the navigable address on success. */
  connect: (id: string) => Promise<string | null>;
  disconnect: (id: string) => Promise<void>;
}

export const useNet = create<NetState>((set, get) => ({
  profiles: [],
  connections: [],
  managerOpen: false,
  editing: null,
  error: null,

  refresh: async () => {
    try {
      const [profiles, connections] = await Promise.all([
        netListProfiles(),
        netListConnections(),
      ]);
      set({ profiles, connections, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  openManager: (keepError = false) => {
    // The refresh below clears `error` on success, so a kept message has to
    // be written back afterwards — otherwise the manager opens blank right
    // after the failure it was opened to explain.
    const kept = keepError ? get().error : null;
    set({ managerOpen: true, editing: null, error: kept });
    void get().refresh().then(() => {
      if (kept && get().managerOpen) set({ error: kept });
    });
  },
  closeManager: () => set({ managerOpen: false, editing: null }),

  startEdit: (profile) => set({ editing: profile, error: null }),
  cancelEdit: () => set({ editing: null }),

  save: async (profile, secret) => {
    try {
      await netSaveProfile(profile, secret);
      set({ editing: null, error: null });
      await get().refresh();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  remove: async (id) => {
    try {
      await netDeleteProfile(id);
      await get().refresh();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  connect: async (id) => {
    try {
      const status = await netConnect(id);
      set({ error: null });
      await get().refresh();
      return status.address;
    } catch (e) {
      set({ error: String(e) });
      return null;
    }
  },

  disconnect: async (id) => {
    try {
      await netDisconnect(id);
      await get().refresh();
      // A comparison describes two reachable folders; losing a
      // connection invalidates it, and its hidden files must come back
      // rather than stay hidden behind a dead session (TICKET-059).
      usePanes.getState().clearCompareFilter();
    } catch (e) {
      set({ error: String(e) });
    }
  },
}));

/** `scheme://authority` of a remote address; null for local paths. */
function authorityOf(path: string): string | null {
  const m = /^([a-z][a-z0-9+.-]*:\/\/[^/]*)/i.exec(path);
  return m ? m[1].toLowerCase() : null;
}

/** The live connection serving `path`, or null (also null for a local
 *  path). Lives here rather than next to the menu commands so that
 *  every caller — menu, pane comparison, transfers — reaches it without
 *  importing the command table. */
export function connectionFor(path: string): ConnectionStatus | null {
  const auth = authorityOf(path);
  if (!auth) return null;
  return (
    useNet
      .getState()
      .connections.find((c) => c.connected && authorityOf(c.address) === auth) ??
    null
  );
}
