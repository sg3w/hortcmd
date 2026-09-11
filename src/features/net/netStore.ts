// ============================================================
// Remote connections (SFTP, ticket 021/022).
//
// Holds the saved connection profiles and their live status, plus the
// manager-dialog state. Secrets never live here — they are passed to
// the backend on save and stored in the OS keychain.
// ============================================================

import { create } from "zustand";
import type { ConnectionStatus, Profile } from "@/ipc/bindings";
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
  openManager: () => void;
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

  openManager: () => {
    set({ managerOpen: true, editing: null, error: null });
    void get().refresh();
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
    } catch (e) {
      set({ error: String(e) });
    }
  },
}));
