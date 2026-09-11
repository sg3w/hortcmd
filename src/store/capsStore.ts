// ============================================================
// Location capabilities in the frontend (TICKET-031).
//
// The backend answers, per address, which operations that location
// supports right now (`location_capabilities`). This store is the one
// place that asks — and the one place every action surface reads:
// command bar, context menu, in-window menu, keyboard shortcuts and the
// dialogs that combine two locations.
//
// Three properties matter and shape the whole module:
//
//   * **One call per location, not per menu item.** Answers are cached
//     under the location's identity (`local`, `sftp:user@host:port`),
//     which is what the backend returns, so every item of a menu reads
//     one cached entry.
//   * **Synchronous reads.** A menu asks for its state while it renders;
//     it cannot await. `capsOf` therefore returns what is known *now*
//     and kicks off the fetch for what is not; the store update re-renders
//     the surfaces that subscribe.
//   * **Invalidated, never stale.** Connecting, disconnecting and
//     reconnecting drop the cached remote answers, so an item enabled
//     before a drop is not authority to run afterwards — every execution
//     path re-asks (`allows`) immediately before it acts.
// ============================================================

import { create } from "zustand";
import type { Capability, LocationCapabilities } from "@/ipc/bindings";
import { locationCapabilities } from "@/ipc/client";
import { schemeOf } from "@/lib/path";

/** Nothing at all — the answer for a location that is not reachable. */
const NONE: readonly Capability[] = [];

/**
 * Cache key of an address: the scheme plus its authority, which is what
 * the backend's `store_id` identifies. Derived locally so a lookup needs
 * no round trip; the backend's own `store_id` overwrites it on arrival,
 * which is why both spellings of one server end up on one entry.
 */
export function locationKey(address: string): string {
  const m = /^([a-z][a-z0-9+.-]*):\/\/([^/]*)/i.exec(address);
  if (!m) return "local";
  const scheme = m[1].toLowerCase();
  return scheme === "file" || scheme === "local"
    ? "local"
    : `${scheme}://${m[2].toLowerCase()}`;
}

interface CapsState {
  /** Answers by location key — both the derived and the backend id. */
  known: Record<string, readonly Capability[]>;
  /** Drops every cached remote answer (connect/disconnect/reconnect). */
  invalidateRemote: () => void;
  /** Drops everything (platform change, hard reload of the backend). */
  invalidateAll: () => void;
}

/**
 * Requests in flight, one per location key. A menu asks a dozen
 * questions about the same folder while it renders and a dialog may ask
 * for the same one again — they all join the request that is already on
 * its way instead of starting another.
 */
const inFlight = new Map<string, Promise<readonly Capability[]>>();

export const useCaps = create<CapsState>((set) => ({
  known: {},
  invalidateRemote: () => {
    inFlight.clear();
    set((s) => {
      const known: Record<string, readonly Capability[]> = {};
      if (s.known.local) known.local = s.known.local;
      return { known };
    });
  },
  invalidateAll: () => {
    inFlight.clear();
    set({ known: {} });
  },
}));

/**
 * Fetches one location's capabilities into the cache, or returns the
 * request already running for it. Errors are not thrown at the caller —
 * an address the backend rejects (unknown scheme, malformed authority)
 * caches as "supports nothing", which is exactly how the UI should
 * present it.
 */
function fetchCaps(
  address: string,
  key: string,
): Promise<readonly Capability[]> {
  const running = inFlight.get(key);
  if (running) return running;
  const request = locationCapabilities(address)
    .then((res: LocationCapabilities) => store(key, res))
    .catch(() => store(key, null))
    .finally(() => inFlight.delete(key));
  inFlight.set(key, request);
  return request;
}

function store(
  key: string,
  res: LocationCapabilities | null,
): readonly Capability[] {
  const caps = res ? res.caps : NONE;
  useCaps.setState((s) => {
    const known = { ...s.known, [key]: caps };
    // The backend's own identity is the authoritative key: two
    // spellings of one server (default port written out or not) share
    // the answer from here on.
    if (res) known[res.store_id] = res.caps;
    return { known };
  });
  return caps;
}

/**
 * Capabilities of `address` as far as they are known right now. An
 * unknown location returns the empty set and starts the fetch, so the
 * first render of a menu shows nothing enabled for a location nobody has
 * asked about yet and settles one update later.
 */
export function capsOf(address: string): readonly Capability[] {
  const key = locationKey(address);
  const known = useCaps.getState().known[key];
  if (known) return known;
  void fetchCaps(address, key);
  return NONE;
}

/**
 * Whether the backend has already answered for this location. Tells a
 * transient "not asked yet" apart from a real "supports nothing", which
 * are two different things to show the user.
 */
export function capsKnown(address: string): boolean {
  return useCaps.getState().known[locationKey(address)] !== undefined;
}

/** Whether `address` supports `cap` right now. */
export function allows(address: string, cap: Capability): boolean {
  return capsOf(address).includes(cap);
}

/** Whether both ends of a two-location operation are supported. */
export function allowsPair(
  from: string,
  fromCap: Capability,
  to: string,
  toCap: Capability,
): boolean {
  return allows(from, fromCap) && allows(to, toCap);
}

/**
 * React hook form: re-renders the component when the answer for
 * `address` arrives or is invalidated. Components that render menus use
 * this; the plain `allows` is for the execution paths, which read once
 * and must not subscribe.
 */
export function useCapability(address: string, cap: Capability): boolean {
  return useCaps((s) => {
    const key = locationKey(address);
    const known = s.known[key];
    if (!known) {
      // Not in the cache yet — start the fetch outside the selector, so
      // no store write happens while React is rendering.
      queueMicrotask(() => capsOf(address));
      return false;
    }
    return known.includes(cap);
  });
}

/** Warms the cache for a location the user just navigated to. */
export function primeCaps(address: string): void {
  capsOf(address);
}

/**
 * Prefetches a location and resolves once the answer is in the cache.
 * Dialogs that combine two locations call this while they open, so their
 * first paint already shows the right enabled/disabled state.
 */
export async function loadCaps(address: string): Promise<readonly Capability[]> {
  const key = locationKey(address);
  const known = useCaps.getState().known[key];
  return known ?? fetchCaps(address, key);
}

/** Human-facing scheme of an address, for the "why not" explanations. */
export function schemeLabel(address: string): string {
  return (schemeOf(address) ?? "local").toUpperCase();
}
