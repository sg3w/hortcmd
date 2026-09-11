// ============================================================
// Shared test environment (TICKET-033).
//
// Every test file starts from the same known state. Three things make
// that true:
//
//   * **A working `localStorage`.** jsdom in this Node build exposes
//     none, and the settings/session stores persist through it — without
//     a shim, importing any store throws before the first assertion.
//   * **Reset between tests.** The DOM, storage, timers, mocks and the
//     Zustand stores are all process-wide. A test that leaves one dirty
//     makes the next one's result depend on file order, which is exactly
//     the flakiness a suite is supposed to catch, not produce.
//   * **No Tauri by default.** `hasTauri` is false in jsdom, so the
//     client's demo fallbacks run unless a test asks for Tauri
//     explicitly (see `src/test/tauri.ts`).
// ============================================================

import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { resetTauri } from "./tauri";

class MemoryStorage implements Storage {
  private data = new Map<string, string>();

  get length(): number {
    return this.data.size;
  }
  clear(): void {
    this.data.clear();
  }
  getItem(key: string): string | null {
    return this.data.get(key) ?? null;
  }
  key(index: number): string | null {
    return [...this.data.keys()][index] ?? null;
  }
  removeItem(key: string): void {
    this.data.delete(key);
  }
  setItem(key: string, value: string): void {
    this.data.set(key, String(value));
  }
}

/** Installs an in-memory `localStorage` where the environment has none. */
function installStorage(): void {
  for (const target of [globalThis, globalThis.window]) {
    if (!target) continue;
    const host = target as unknown as { localStorage?: Storage };
    if (!host.localStorage || !(host.localStorage instanceof MemoryStorage)) {
      Object.defineProperty(host, "localStorage", {
        value: new MemoryStorage(),
        configurable: true,
        writable: true,
      });
    }
  }
}

// The stores are created when a test file imports them, which happens
// before the first `beforeEach` — so the shim goes in at module load,
// not only between tests.
installStorage();

// ---------- Zustand ----------

/**
 * Stores are module-level singletons: a test that connects a profile,
 * opens a dialog or moves a cursor would leave that visible to every
 * later test in the same worker. Wrapping `create`/`createStore` records
 * each store's initial state, and the reset below puts it back — which
 * is what makes the suite independent of file and test order.
 *
 * `zustand/middleware` (persist) keeps resolving to the real package;
 * only the bare specifier is replaced.
 */
const resets: (() => void)[] = [];

vi.mock("zustand", async () => {
  const actual = await vi.importActual<typeof import("zustand")>("zustand");

  type AnyStore = {
    getState: () => unknown;
    setState: (state: never, replace: true) => void;
  };
  type Factory = (creator?: unknown) => unknown;

  const track = (store: AnyStore): AnyStore => {
    const initial = store.getState();
    resets.push(() => store.setState(initial as never, true));
    return store;
  };

  // Both the curried (`create()(fn)`) and direct (`create(fn)`) forms.
  const wrap =
    (real: Factory): Factory =>
    (creator?: unknown) =>
      creator === undefined
        ? (later: unknown) => track(real(later) as AnyStore)
        : track(real(creator) as AnyStore);

  return {
    ...actual,
    create: wrap(actual.create as unknown as Factory),
    createStore: wrap(actual.createStore as unknown as Factory),
  };
});

// ---------- Lifecycle ----------

beforeEach(() => {
  installStorage();
  // jsdom keeps no per-test window; anything a previous test measured
  // or observed has to go.
  vi.useRealTimers();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.clearAllTimers();
  vi.useRealTimers();
  resetTauri();
  for (const reset of resets) reset();
  globalThis.localStorage?.clear();
  document.body.innerHTML = "";
});
