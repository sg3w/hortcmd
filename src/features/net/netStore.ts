// ============================================================
// Remote connections (SFTP, ticket 021/022) + credential backend
// (ticket 027).
//
// Holds the saved connection profiles and their live status, plus the
// manager-dialog state. Secrets never live here — they are passed to
// the backend on save and land in the OS keychain or, when the user
// picks it, in a KeePass file.
//
// A KeePass file can be locked. Rather than failing the click, every
// credential-touching action parks itself behind the unlock prompt and
// is replayed once the master password arrives.
// ============================================================

import { create } from "zustand";
import type {
  ConnectionStatus,
  CredConfig,
  CredStatus,
  Profile,
} from "@/ipc/bindings";
import { usePanes } from "@/store/panesStore";
import {
  netConnect,
  netCredLock,
  netCredSetBackend,
  netCredStatus,
  netCredUnlock,
  netDeleteProfile,
  netDisconnect,
  netListConnections,
  netListProfiles,
  netSaveProfile,
} from "@/ipc/client";

/** Marker the backend puts on every "KeePass file is locked" error. */
const LOCKED = "KEEPASS_LOCKED";

function isLocked(e: unknown): boolean {
  return String(e).includes(LOCKED);
}

/** Which panel the manager shows; the edit form rides on `editing`. */
export type NetPanel = "list" | "store" | "unlock";

interface NetState {
  profiles: Profile[];
  connections: ConnectionStatus[];
  managerOpen: boolean;
  /** Profile currently being edited in the form, or null for the list view. */
  editing: Profile | null;
  error: string | null;

  // ----- Credential backend (ticket 027) -----
  /** Backend choice + lock state; null until the first refresh. */
  cred: CredStatus | null;
  panel: NetPanel;
  /** Action parked behind the unlock prompt, replayed after unlocking. */
  pending: (() => Promise<void>) | null;

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

  showPanel: (panel: NetPanel) => void;
  refreshCred: () => Promise<void>;
  /** Switches the credential backend; false when the backend refused. */
  setCredBackend: (config: CredConfig) => Promise<boolean>;
  credUnlock: (password: string, remember: boolean, create: boolean) => Promise<void>;
  credLock: () => Promise<void>;
}

export const useNet = create<NetState>((set, get) => ({
  profiles: [],
  connections: [],
  managerOpen: false,
  editing: null,
  error: null,
  cred: null,
  panel: "list",
  pending: null,

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
    set({ managerOpen: true, editing: null, panel: "list", error: kept });
    void get().refresh().then(() => {
      if (kept && get().managerOpen) set({ error: kept });
    });
    void get().refreshCred();
  },
  closeManager: () =>
    set({ managerOpen: false, editing: null, panel: "list", pending: null }),

  startEdit: (profile) => set({ editing: profile, error: null }),
  cancelEdit: () => set({ editing: null }),

  save: async (profile, secret) => {
    await park(set, get, async () => {
      await netSaveProfile(profile, secret);
      set({ editing: null, error: null });
      await get().refresh();
    });
  },

  remove: async (id) => {
    await park(set, get, async () => {
      await netDeleteProfile(id);
      await get().refresh();
    });
  },

  connect: async (id) => {
    try {
      const status = await netConnect(id);
      set({ error: null });
      await get().refresh();
      return status.address;
    } catch (e) {
      if (!isLocked(e)) {
        set({ error: String(e) });
        return null;
      }
      // Park the *whole* step, navigation included — by the time the
      // master password arrives, the caller has long given up.
      await park(set, get, async () => {
        const address = await get().connect(id);
        if (!address) return;
        const panes = usePanes.getState();
        get().closeManager();
        void panes.loadDir(panes.active, address);
      });
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

  // ----- Credential backend (ticket 027) -----

  showPanel: (panel) => set({ panel, editing: null, error: null }),

  refreshCred: async () => {
    try {
      set({ cred: await netCredStatus() });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setCredBackend: async (config) => {
    try {
      set({ cred: await netCredSetBackend(config), error: null });
      return true;
    } catch (e) {
      set({ error: String(e) });
      return false;
    }
  },

  credUnlock: async (password, remember, create) => {
    let cred: CredStatus;
    try {
      cred = await netCredUnlock(password, remember, create);
    } catch (e) {
      set({ error: String(e) });
      return;
    }
    // Replay what the lock interrupted. It may park itself again — a
    // one-shot unlock deliberately covers exactly one access.
    const pending = get().pending;
    set({ cred, error: null, pending: null, panel: "list" });
    if (pending) await pending();
  },

  credLock: async () => {
    try {
      set({ cred: await netCredLock(), error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },
}));

/**
 * Runs a credential-touching action. A locked KeePass file is not a
 * failure but a question: the action is parked, the unlock prompt comes
 * up, and `credUnlock` replays it.
 */
async function park(
  set: (partial: Partial<NetState>) => void,
  get: () => NetState,
  run: () => Promise<void>,
): Promise<void> {
  try {
    await run();
  } catch (e) {
    if (!isLocked(e)) {
      set({ error: String(e) });
      return;
    }
    set({ managerOpen: true, panel: "unlock", pending: run, error: null });
    void get().refreshCred();
  }
}

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
