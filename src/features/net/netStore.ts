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
  ForgetResult,
  HostKeyChallenge,
  Profile,
} from "@/ipc/bindings";
import { describeConnError } from "./connFailure";
import { isError } from "@/lib/appError";
import { describeErrorFull } from "@/store/errorText";
import { useCaps } from "@/store/capsStore";
import { usePanes } from "@/store/panesStore";
import {
  netCancelConnect,
  netConnect,
  netCredLock,
  netCredSetBackend,
  netCredStatus,
  netCredUnlock,
  netDeleteProfile,
  netDisconnect,
  netForgetHostKey,
  netListConnections,
  netListProfiles,
  netSaveProfile,
  netTrustHostKey,
} from "@/ipc/client";

/**
 * "The KeePass file is locked" — a question, not a failure: the action
 * is parked behind the unlock prompt and replayed once the master
 * password arrives. Recognized by its code since TICKET-036; it used to
 * be a marker searched for inside a sentence.
 */
const LOCKED = "cred.locked";

/** Swaps one profile's status into the list (or appends it). */
function replaceStatus(
  list: ConnectionStatus[],
  next: ConnectionStatus,
): ConnectionStatus[] {
  const idx = list.findIndex((c) => c.id === next.id);
  if (idx < 0) return [...list, next];
  const copy = [...list];
  copy[idx] = next;
  return copy;
}

/** Which connections are live — the part a capability answer depends on. */
function liveSignature(connections: ConnectionStatus[]): string {
  return connections
    .filter((c) => c.connected)
    .map((c) => c.address)
    .sort()
    .join("|");
}

function isLocked(e: unknown): boolean {
  return isError(e, LOCKED);
}

/** Which panel the manager shows; the edit form rides on `editing`. */
export type NetPanel = "list" | "store" | "unlock" | "hostkey" | "forget";

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

  // ----- SSH host keys (ticket 028) -----
  /** Host-key question raised by the last connect attempt. */
  hostKey: HostKeyChallenge | null;
  /** Profile the challenge belongs to — the retry needs its secret. */
  hostKeyProfile: string | null;
  /** Profile whose stored host keys the user is about to drop. */
  forgetting: Profile | null;
  /** Result of the last `forgetHostKey`, shown as a confirmation. */
  forgot: ForgetResult | null;

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
  /** Stops a running attempt (TICKET-032); idempotent. */
  cancelConnect: (id: string) => Promise<void>;
  /** Replaces the connection list from a backend `net-state` push. */
  applyStates: (states: ConnectionStatus[]) => void;

  showPanel: (panel: NetPanel) => void;
  refreshCred: () => Promise<void>;
  /** Switches the credential backend; false when the backend refused. */
  setCredBackend: (config: CredConfig) => Promise<boolean>;
  credUnlock: (password: string, remember: boolean, create: boolean) => Promise<void>;
  credLock: () => Promise<void>;

  /** Persists the offered host key and retries; returns the address. */
  trustHostKey: () => Promise<string | null>;
  /** Drops the challenge without trusting anything. */
  dismissHostKey: () => void;
  /** Opens the "drop stored host keys" confirmation for a profile. */
  startForget: (profile: Profile) => void;
  /** Carries it out; the user's own known_hosts files stay untouched. */
  forgetHostKey: () => Promise<void>;
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
  hostKey: null,
  hostKeyProfile: null,
  forgetting: null,
  forgot: null,

  refresh: async () => {
    try {
      const [profiles, connections] = await Promise.all([
        netListProfiles(),
        netListConnections(),
      ]);
      // Connecting, disconnecting and reconnecting change what the
      // remote locations support, so the cached capability answers go
      // (TICKET-031). Compared rather than dropped unconditionally:
      // `refresh` also runs when the manager merely opens.
      if (liveSignature(connections) !== liveSignature(get().connections)) {
        useCaps.getState().invalidateRemote();
      }
      set({ profiles, connections, error: null });
    } catch (e) {
      set({ error: describeErrorFull(e) });
    }
  },

  openManager: (keepError = false) => {
    // The refresh below clears `error` on success, so a kept message has to
    // be written back afterwards — otherwise the manager opens blank right
    // after the failure it was opened to explain.
    const kept = keepError ? get().error : null;
    // A pending host-key question outlives the reopen — the menu opens the
    // manager right after the connect attempt that raised it.
    const panel: NetPanel = get().hostKey ? "hostkey" : "list";
    set({ managerOpen: true, editing: null, panel, error: kept });
    void get().refresh().then(() => {
      if (kept && get().managerOpen) set({ error: kept });
    });
    void get().refreshCred();
  },
  closeManager: () =>
    set({
      managerOpen: false,
      editing: null,
      panel: "list",
      pending: null,
      hostKey: null,
      hostKeyProfile: null,
      forgetting: null,
      forgot: null,
    }),

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
      const outcome = await netConnect(id);
      if (outcome.kind === "pending") {
        // A duplicate request. The attempt that is already running owns
        // the profile; adopting its state is all there is to do here
        // (TICKET-032) — no second socket, no second error.
        set({ connections: replaceStatus(get().connections, outcome.data) });
        return null;
      }
      if (outcome.kind === "hostKey") {
        // Nothing was authenticated — the server identity has to be
        // settled first (ticket 028).
        set({
          hostKey: outcome.data,
          hostKeyProfile: id,
          managerOpen: true,
          panel: "hostkey",
          editing: null,
          error: null,
        });
        return null;
      }
      set({ error: null, hostKey: null, hostKeyProfile: null });
      await get().refresh();
      return outcome.data.address;
    } catch (e) {
      if (!isLocked(e)) {
        // A typed lifecycle failure reads as one localized sentence
        // (TICKET-032); anything else is passed through unchanged.
        set({ error: describeConnError(e) });
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

  cancelConnect: async (id) => {
    const attempt = get().connections.find((c) => c.id === id)?.attempt;
    if (!attempt) return;
    try {
      await netCancelConnect(attempt);
      await get().refresh();
    } catch (e) {
      // Cancelling is idempotent in the backend, so a failure here means
      // the IPC itself broke — worth showing, never worth blocking on.
      set({ error: describeErrorFull(e) });
    }
  },

  applyStates: (states) => {
    if (liveSignature(states) !== liveSignature(get().connections)) {
      useCaps.getState().invalidateRemote();
    }
    set({ connections: states });
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
      set({ error: describeErrorFull(e) });
    }
  },

  // ----- Credential backend (ticket 027) -----

  showPanel: (panel) => set({ panel, editing: null, error: null }),

  refreshCred: async () => {
    try {
      set({ cred: await netCredStatus() });
    } catch (e) {
      set({ error: describeErrorFull(e) });
    }
  },

  setCredBackend: async (config) => {
    try {
      set({ cred: await netCredSetBackend(config), error: null });
      return true;
    } catch (e) {
      set({ error: describeErrorFull(e) });
      return false;
    }
  },

  credUnlock: async (password, remember, create) => {
    let cred: CredStatus;
    try {
      cred = await netCredUnlock(password, remember, create);
    } catch (e) {
      set({ error: describeErrorFull(e) });
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
      set({ error: describeErrorFull(e) });
    }
  },

  // ----- SSH host keys (ticket 028) -----

  trustHostKey: async () => {
    const { hostKey, hostKeyProfile } = get();
    if (!hostKey?.token || !hostKeyProfile) return null;
    try {
      const outcome = await netTrustHostKey(hostKeyProfile, hostKey.token);
      if (outcome.kind === "hostKey") {
        // The server answered with a different key than the one shown.
        set({ hostKey: outcome.data, panel: "hostkey", error: null });
        return null;
      }
      set({ hostKey: null, hostKeyProfile: null, panel: "list", error: null });
      await get().refresh();
      return outcome.data.address;
    } catch (e) {
      set({ error: describeErrorFull(e), hostKey: null, hostKeyProfile: null, panel: "list" });
      return null;
    }
  },

  dismissHostKey: () =>
    set({ hostKey: null, hostKeyProfile: null, panel: "list" }),

  startForget: (profile) =>
    set({ forgetting: profile, forgot: null, panel: "forget", error: null }),

  forgetHostKey: async () => {
    const profile = get().forgetting;
    if (!profile) return;
    try {
      set({ forgot: await netForgetHostKey(profile.id), error: null });
    } catch (e) {
      set({ error: describeErrorFull(e), forgetting: null, panel: "list" });
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
      set({ error: describeErrorFull(e) });
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
