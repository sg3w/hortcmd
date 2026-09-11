// ============================================================
// Search (TICKET-035).
//
// One long-running, cancellable command that streams its matches over a
// channel; the summary comes back when it ends.
// ============================================================

import type {
  SearchHit,
  SearchSummary,
} from "../bindings";
import { hasTauri, invoke } from "../invoke";
import * as demo from "../demo";
import type { SearchOptions } from "../types";

/**
 * Searches `root` and streams matches in batches. `token` identifies the
 * run for `cancelSearch`. Only `status: "complete"` means the returned
 * list is the full answer – see `SearchStatus`.
 */
export async function search(
  root: string,
  options: SearchOptions,
  token: string,
  onBatch: (hits: SearchHit[]) => void,
): Promise<SearchSummary> {
  if (!hasTauri) {
    const rows = demo.demoSearch(options);
    onBatch(rows);
    return {
      status: "complete",
      hits: rows.length,
      files: rows.length,
      dirs: 0,
      unreadable: 0,
    };
  }
  const { Channel } = await import("@tauri-apps/api/core");
  const channel = new Channel<SearchHit[]>();
  channel.onmessage = onBatch;
  return invoke<SearchSummary>("search", { root, options, token, onBatch: channel });
}

/** Stops a running `search`; unknown tokens are ignored. */
export async function cancelSearch(token: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("cancel_search", { token });
}
