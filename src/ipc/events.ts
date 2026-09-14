// ============================================================
// Event routing (TICKET-035).
//
// The backend pushes progress, folder changes, terminal output, Git
// results, the connection lifecycle and native-menu clicks. All of it
// arrives on one Tauri listener set, wired once on the first
// subscription, and fans out to the subscribers registered here.
//
// The demo simulator feeds the same subscriber sets, which is what lets
// the stores be written without knowing which mode they are in.
// ============================================================

import type {
  CollisionReq,
  Drive,
  ConnectionStatus,
  GitStatus,
  GitStatusEvent,
  Leftovers,
  OpDone,
  OpProgress,
  TermClosed,
  TermData,
} from "./bindings";
import { hasTauri } from "./invoke";

export const progressSubs = new Set<(p: OpProgress) => void>();

export const doneSubs = new Set<(d: OpDone) => void>();

export const collisionSubs = new Set<(c: CollisionReq) => void>();

export const drivesSubs = new Set<(drives: Drive[]) => void>();

export const changedSubs = new Set<(dir: string) => void>();

export const leftoverSubs = new Set<(l: Leftovers) => void>();

export const gitReadySubs = new Set<(path: string, status: GitStatus) => void>();

export const termDataSubs = new Set<(d: TermData) => void>();

export const termClosedSubs = new Set<(c: TermClosed) => void>();

export const nativeMenuSubs = new Set<(id: string) => void>();

export const netStateSubs = new Set<(states: ConnectionStatus[]) => void>();

let tauriReady: Promise<void> | null = null;

export function ensureTauriEvents(): Promise<void> {
  if (!hasTauri) return Promise.resolve();
  if (!tauriReady) tauriReady = wireEvents().catch(error => { tauriReady = null; throw error; });
  return tauriReady;
}

async function wireEvents(): Promise<void> {
  const { listen } = await import("@tauri-apps/api/event");
  const results = await Promise.allSettled([
    listen<Drive[]>("drives-changed", e => drivesSubs.forEach(cb => cb(e.payload))),
    listen<OpProgress>("fs-progress", (e) =>
    progressSubs.forEach((f) => f(e.payload)),
    ),
    listen<OpDone>("fs-done", (e) => doneSubs.forEach((f) => f(e.payload))),
    listen<CollisionReq>("fs-collision", (e) =>
    collisionSubs.forEach((f) => f(e.payload)),
    ),
    listen<string>("fs-changed", (e) =>
    changedSubs.forEach((f) => f(e.payload)),
    ),
    listen<Leftovers>("fs-leftovers", (e) =>
    leftoverSubs.forEach((f) => f(e.payload)),
    ),
    listen<GitStatusEvent>("git-support-ready", (e) =>
    gitReadySubs.forEach((f) => f(e.payload.path, e.payload.status)),
    ),
    listen<TermData>("ssh-term-data", (e) =>
    termDataSubs.forEach((f) => f(e.payload)),
    ),
    listen<TermClosed>("ssh-term-closed", (e) =>
    termClosedSubs.forEach((f) => f(e.payload)),
    ),
    listen<string>("native-menu", (e) =>
    nativeMenuSubs.forEach((f) => f(e.payload)),
    ),
    listen<ConnectionStatus[]>("net-state", (e) =>
    netStateSubs.forEach((f) => f(e.payload)),
    ),
  ]);
  const failure = results.find(r => r.status === "rejected");
  if (failure?.status === "rejected") {
    for (const result of results) if (result.status === "fulfilled") result.value();
    throw failure.reason;
  }
}

/** Subscribes to the remains of interrupted failure-safe transfers found
 *  in a destination folder (TICKET-029). They are only reported — the
 *  frontend asks before anything is deleted. */
export function onLeftovers(cb: (l: Leftovers) => void): () => void {
  leftoverSubs.add(cb);
  void ensureTauriEvents();
  return () => leftoverSubs.delete(cb);
}

/**
 * Subscribes to the authoritative connection lifecycle (TICKET-032).
 * The backend pushes the whole list on every transition — attempt
 * started, cancelled, finished, disconnected, connection lost — so the
 * manager and the location pickers can never disagree about who is
 * connected.
 */
export function onNetState(cb: (states: ConnectionStatus[]) => void): () => void {
  netStateSubs.add(cb);
  void ensureTauriEvents();
  return () => netStateSubs.delete(cb);
}

/** Subscribes to clicks in the reduced native macOS menu (TICKET-040). */
export function onNativeMenu(cb: (id: string) => void): () => void {
  nativeMenuSubs.add(cb);
  void ensureTauriEvents();
  return () => nativeMenuSubs.delete(cb);
}

/** Subscribes to shell output (base64 chunks) of all SSH terminals. */
export function onSshTermData(cb: (d: TermData) => void): () => void {
  termDataSubs.add(cb);
  void ensureTauriEvents();
  return () => termDataSubs.delete(cb);
}

/** Subscribes to the end of an SSH terminal session. */
export function onSshTermClosed(cb: (c: TermClosed) => void): () => void {
  termClosedSubs.add(cb);
  void ensureTauriEvents();
  return () => termClosedSubs.delete(cb);
}

export function onFsProgress(cb: (p: OpProgress) => void): () => void {
  progressSubs.add(cb);
  void ensureTauriEvents();
  return () => progressSubs.delete(cb);
}

export function onFsDone(cb: (d: OpDone) => void): () => void {
  doneSubs.add(cb);
  void ensureTauriEvents();
  return () => doneSubs.delete(cb);
}

export function onFsCollision(cb: (c: CollisionReq) => void): () => void {
  collisionSubs.add(cb);
  void ensureTauriEvents();
  return () => collisionSubs.delete(cb);
}

/** Subscribes to external folder changes (directory watcher). */
export function onFsChanged(cb: (dir: string) => void): () => void {
  changedSubs.add(cb);
  void ensureTauriEvents();
  return () => changedSubs.delete(cb);
}

/** Subscribes to Git status results (see requestGitStatus). */
export function onGitStatusReady(
  cb: (path: string, status: GitStatus) => void,
): () => void {
  gitReadySubs.add(cb);
  void ensureTauriEvents();
  return () => gitReadySubs.delete(cb);
}

export function onDrivesChanged(cb: (drives: Drive[]) => void): () => void {
  drivesSubs.add(cb);
  void ensureTauriEvents().catch(() => {});
  return () => { drivesSubs.delete(cb); };
}
