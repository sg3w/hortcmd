// ============================================================
// Location capabilities (TICKET-031, split out in TICKET-035).
//
// What the location behind an address supports right now. One call per
// location; the frontend caches the answer (see `store/capsStore.ts`).
// ============================================================

import type {
  LocationCapabilities,
} from "../bindings";
import { hasTauri, invoke } from "../invoke";
import * as demo from "../demo";

/**
 * What the location behind `address` supports right now. Resolved
 * through the same parser and session registry as the file-system
 * commands, so the answer describes the location the commands would
 * actually act on — including "nothing" for a dropped connection.
 *
 * The browser demo has no backend and therefore one deterministic
 * answer: the local set, with the same platform gates the Rust matrix
 * applies (Quick Look and Finder tags on macOS, rights editing on
 * Unix — the demo counts macOS as Unix, which is what it runs on).
 */
export async function locationCapabilities(
  address: string,
): Promise<LocationCapabilities> {
  if (!hasTauri) return demo.demoCapabilities();
  return invoke<LocationCapabilities>("location_capabilities", { address });
}
