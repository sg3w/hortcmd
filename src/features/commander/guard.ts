// ============================================================
// The one place an action asks "may this location do that?"
// (TICKET-031).
//
// Menus, the command bar and the keyboard all end up here. A surface
// that renders asks `capState` for the disabled state and the
// explanation; every execution path calls `guard` immediately before it
// acts — the folder may have changed or a connection dropped since the
// item was drawn, and an enabled item from before a disconnect is not
// authorization to run.
//
// A blocked keyboard shortcut must not be a silent no-op, so `guard`
// raises the same explanation as a pane notice.
// ============================================================

import type { Capability } from "@/ipc/bindings";
import type { TransKey } from "@/i18n/dictionaries";
import {
  allows,
  capsKnown,
  capsOf,
  loadCaps,
  schemeLabel,
} from "@/store/capsStore";
import { fillVars, notify } from "@/store/noticeStore";
import { translate } from "@/store/settingsStore";
import { schemeOf } from "@/lib/path";
import type { Side } from "@/store/panesStore";

/** Disabled state and explanation, in the shape the menus use. */
export interface CapState {
  disabled?: boolean;
  reason?: TransKey;
  reasonVars?: Record<string, string>;
}

/**
 * Why a location cannot do something: a remote location that supports
 * nothing at all has lost its connection; one that supports other things
 * simply does not implement this operation.
 */
function why(address: string, forDestination: boolean): CapState {
  // `capsOf` starts the fetch for a location nobody has asked about
  // yet; until it answers, "not yet known" is the honest state — not
  // "unsupported", which would be a wrong message for one frame.
  const caps = capsOf(address);
  if (!capsKnown(address)) {
    return { disabled: true, reason: "menu.why.checking" };
  }
  const scheme = schemeOf(address);
  if (scheme !== null && caps.length === 0) {
    return { disabled: true, reason: "menu.why.offline" };
  }
  return {
    disabled: true,
    reason: forDestination
      ? "menu.why.destUnsupported"
      : "menu.why.unsupported",
    reasonVars: { scheme: schemeLabel(address) },
  };
}

/**
 * Whether the location behind `address` is reachable at all: a remote
 * one needs a live session, a local one always is. The single "is this
 * folder still there" test the command table uses — it reads the same
 * contract as every capability question, so a menu cannot disagree with
 * itself about one location.
 */
export function locationLive(address: string): boolean {
  return schemeOf(address) === null || capsOf(address).length > 0;
}

/** Disabled state for a single location and capability. */
export function capState(address: string, cap: Capability): CapState {
  return allows(address, cap) ? {} : why(address, false);
}

/**
 * Disabled state for an operation that spans two locations (copy, move,
 * pack, extract, compare, synchronize). Source and destination are asked
 * independently — an allowed source is no licence for the target.
 */
export function pairState(
  from: string,
  fromCap: Capability,
  to: string,
  toCap: Capability,
): CapState {
  if (!allows(from, fromCap)) return why(from, false);
  if (!allows(to, toCap)) return why(to, true);
  return {};
}

/**
 * Gate in front of an execution path. Returns true when the action may
 * run; otherwise shows the explanation in `side`'s pane and returns
 * false — the same wording a disabled menu item carries.
 */
export function guard(side: Side, address: string, cap: Capability): boolean {
  const st = capState(address, cap);
  if (!st.disabled) return true;
  if (st.reason) notify(side, st.reason, st.reasonVars, "error");
  return false;
}

/** Same gate for a two-location operation. */
export function guardPair(
  side: Side,
  from: string,
  fromCap: Capability,
  to: string,
  toCap: Capability,
): boolean {
  const st = pairState(from, fromCap, to, toCap);
  if (!st.disabled) return true;
  if (st.reason) notify(side, st.reason, st.reasonVars, "error");
  return false;
}

/**
 * The generic "this location cannot do that" wording for `address` —
 * one sentence a whole menu can share instead of asking per item.
 */
export function locationReason(address: string): string {
  const st = why(address, false);
  return st.reason ? fillVars(translate(st.reason), st.reasonVars) : "";
}

/**
 * Localized explanation for a location that cannot run `cap`, or null
 * when it can. Dialogs use it for their inline status line, where a
 * disabled button alone would not say why.
 */
export function explain(address: string, cap: Capability): string | null {
  const st = capState(address, cap);
  if (!st.disabled || !st.reason) return null;
  return fillVars(translate(st.reason), st.reasonVars);
}

/**
 * Same for a two-location operation, but fetches both answers first —
 * a dialog opens with locations nobody has asked about yet, and its
 * first paint should already be right. Returns null when both ends are
 * supported.
 */
export async function explainPair(
  from: string,
  fromCap: Capability,
  to: string,
  toCap: Capability,
): Promise<string | null> {
  await Promise.all([loadCaps(from), loadCaps(to)]);
  return explain(from, fromCap) ?? explain(to, toCap);
}
