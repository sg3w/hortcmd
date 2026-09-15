// ============================================================
// Typed Tauri stand-ins for tests (TICKET-033).
//
// The IPC client answers in two modes: with a Tauri runtime it invokes
// commands and listens for events; without one it falls back to demo
// data for the browser build. Both are real behavior and both need to
// be testable, so a test says which one it wants rather than getting
// whichever the environment happens to provide.
//
//   * `useDemo()` — no runtime. `hasTauri` is false, the demo fallbacks
//     run, and no command may be invoked. This is the default.
//   * `useTauri({ commands })` — a runtime whose commands answer from
//     the supplied table. Calls are recorded, an unhandled command
//     fails loudly rather than returning undefined, and events can be
//     pushed with `emit`.
//
// Neither mode starts a process: everything here is in-memory, so the
// frontend suite runs in CI with no native side at all.
// ============================================================

import { vi } from "vitest";

/** One recorded IPC call. */
export interface InvokeCall {
  cmd: string;
  args: Record<string, unknown>;
}

/** A command implementation: returns the payload, or throws to reject. */
export type CommandImpl = (args: Record<string, unknown>) => unknown;

interface TauriHarness {
  /** Every command invoked since the mode was selected, in order. */
  calls: InvokeCall[];
  /** Pushes an event payload to everything listening for `name`. */
  emit: (name: string, payload: unknown) => void;
  /** Replaces or adds a command after the fact. */
  setCommand: (cmd: string, impl: CommandImpl) => void;
}

type Listener = (event: { payload: unknown }) => void;

const listeners = new Map<string, Set<Listener>>();
let commands: Record<string, CommandImpl> = {};
let calls: InvokeCall[] = [];

const INTERNALS = "__TAURI_INTERNALS__";

/** `invoke` as the client sees it, backed by the command table. */
async function invoke(cmd: string, args?: Record<string, unknown>) {
  const call: InvokeCall = { cmd, args: args ?? {} };
  calls.push(call);
  const impl = commands[cmd];
  if (!impl) {
    // Silence here would let a test pass while the real command was
    // never reached — the opposite of what the mock is for.
    throw new Error(
      `No mock for the Tauri command "${cmd}". Add it to useTauri({ commands }).`,
    );
  }
  return impl(call.args);
}

async function listen(name: string, cb: Listener) {
  let set = listeners.get(name);
  if (!set) {
    set = new Set();
    listeners.set(name, set);
  }
  set.add(cb);
  return () => set.delete(cb);
}

/** Makes `@tauri-apps/api/*` resolve to the harness instead of the real thing. */
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class<T> {
  // Commands can stream typed batches through the same callback as Tauri.
  onmessage: (message: T) => void = () => {};
} }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

/**
 * Runs the test against a Tauri runtime whose commands answer from
 * `commands`. Returns the harness for assertions and event pushes.
 */
export function useTauri(options: {
  commands?: Record<string, CommandImpl>;
} = {}): TauriHarness {
  commands = { ...options.commands };
  calls = [];
  (globalThis as Record<string, unknown>)[INTERNALS] = { invoke };
  return {
    get calls() {
      return calls;
    },
    emit: (name, payload) =>
      listeners.get(name)?.forEach((cb) => cb({ payload })),
    setCommand: (cmd, impl) => {
      commands[cmd] = impl;
    },
  };
}

/**
 * Runs the test as the browser build does: no runtime, so the client's
 * demo fallbacks answer and no command may be invoked. This is the
 * state every test starts in.
 */
export function useDemo(): void {
  delete (globalThis as Record<string, unknown>)[INTERNALS];
  commands = {};
  calls = [];
}

/** Drops every command, listener and recorded call (runs after each test). */
export function resetTauri(): void {
  useDemo();
  listeners.clear();
}

/**
 * Whether anything is currently listening for `name` — the way to check
 * that a subscription was set up, and that unsubscribing removed it.
 */
export function listenerCount(name: string): number {
  return listeners.get(name)?.size ?? 0;
}
