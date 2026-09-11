// ============================================================
// SSH terminal session (ticket 026).
//
// Holds the shell session of the terminal window. A session is created
// here from a remote address; the backend id only arrives once the
// terminal component knows its size (the shell needs cols/rows on
// open), so `sshId` is filled in afterwards via `attach`. The terminal
// window is a modal dialog, so exactly one session exists at a time.
// ============================================================

import { create } from "zustand";
import { sshClose } from "@/ipc/client";
import { schemeOf } from "@/lib/path";

export type TermStatus = "opening" | "open" | "closed";

export interface TermSession {
  /** Local id — stable for React, independent of the backend session. */
  uid: string;
  /** Remote VFS address the shell was opened for. */
  address: string;
  /** Short label for the title bar (`user@host:/path`). */
  title: string;
  /** Backend session id; null until the shell has been opened. */
  sshId: string | null;
  status: TermStatus;
  /** Why the session ended (remote exit, connection loss, error). */
  reason: string | null;
}

interface TermState {
  session: TermSession | null;

  /** Opens the terminal window with a shell for `address`. */
  openFor: (address: string) => void;
  /** Binds the backend session id to the session. */
  attach: (uid: string, sshId: string) => void;
  /** The shell could not be opened. */
  fail: (uid: string, reason: string) => void;
  /** The session ended remotely (backend event). */
  markClosed: (sshId: string, reason: string) => void;
  /** Closes the session and the terminal window. */
  close: () => void;
}

/** `user@host:/path` — short enough for the title bar. */
function labelOf(address: string): string {
  const m = /^[a-z][a-z0-9+.-]*:\/\/([^/]*)(\/.*)?$/i.exec(address);
  if (!m) return address;
  const host = m[1].replace(/:\d+$/, "");
  return `${host}:${m[2] ?? "/"}`;
}

let counter = 0;

/** Applies `patch` only when the session is still the expected one. */
function patchSession(
  session: TermSession | null,
  matches: (s: TermSession) => boolean,
  patch: Partial<TermSession>,
): TermSession | null {
  return session && matches(session) ? { ...session, ...patch } : session;
}

export const useTerminals = create<TermState>((set, get) => ({
  session: null,

  openFor: (address) => {
    // A previous session cannot normally still be open (modal window),
    // but never leak one if it is.
    get().close();
    set({
      session: {
        uid: `t${++counter}`,
        address,
        title: labelOf(address),
        sshId: null,
        status: "opening",
        reason: null,
      },
    });
  },

  attach: (uid, sshId) =>
    set((s) => ({
      session: patchSession(s.session, (t) => t.uid === uid, {
        sshId,
        status: "open",
      }),
    })),

  fail: (uid, reason) =>
    set((s) => ({
      session: patchSession(s.session, (t) => t.uid === uid, {
        status: "closed",
        reason,
      }),
    })),

  markClosed: (sshId, reason) =>
    set((s) => ({
      session: patchSession(s.session, (t) => t.sshId === sshId, {
        status: "closed",
        reason,
      }),
    })),

  close: () => {
    const sshId = get().session?.sshId;
    if (sshId) void sshClose(sshId);
    set({ session: null });
  },
}));

/** Whether an address is served by an SSH connection (SFTP → shell). */
export function supportsSshTerminal(address: string): boolean {
  return schemeOf(address) === "sftp";
}
