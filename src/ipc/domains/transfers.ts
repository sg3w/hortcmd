// ============================================================
// Copy, move, extract and pack (TICKET-035).
//
// These start background work and return immediately: the result
// arrives as `fs-progress` / `fs-done` events, which `events.ts`
// routes. Cancel, pause and the collision answer address a running
// operation by its id.
// ============================================================

import { hasTauri, invoke } from "../invoke";
import { ensureTauriEvents, doneSubs } from "../events";
import { ipcError } from "@/lib/appError";
import * as demo from "../demo";

/** Starts a copy/move transfer (fire-and-forget).
 *  `limit` = bytes/s (0 = unlimited), `verify` = checksums after copying,
 *  `safe` = stage and commit instead of writing to the final name
 *  (TICKET-029). The caller passes the value captured when the transfer
 *  was queued, so a waiting operation keeps the policy it was started with. */
export function startTransfer(
  op: "copy" | "move",
  id: string,
  sources: string[],
  destDir: string,
  limit = 0,
  verify = false,
  bufKb = 256,
  threads = 1,
  safe = false,
): void {
  if (!hasTauri) {
    demo.demoTransfer(op, id, sources, destDir, true);
    return;
  }
  const cmd = op === "copy" ? "copy_entries" : "move_entries";
  void ensureTauriEvents().then(() => invoke<void>(cmd, {
    id,
    sources,
    destDir,
    limit,
    verify,
    bufKb,
    threads,
    safe,
  })).catch(error => failed(id, op, error));
}

/** Pauses or resumes a running transfer. */
export function pauseTransfer(id: string, paused: boolean): void {
  if (!hasTauri) {
    demo.demoPause(id, paused);
    return;
  }
  void invoke<void>("pause_transfer", { id, paused });
}

/** Extracts entries from an archive (fire-and-forget). */
export function startExtract(
  id: string,
  archive: string,
  base: string,
  names: string[],
  destDir: string,
  password?: string,
): void {
  if (!hasTauri) {
    demo.demoTransfer("extract", id, names.length ? names : ["(alles)"], destDir, false);
    return;
  }
  void ensureTauriEvents().then(() => invoke<void>("extract_entries", {
    id,
    archive,
    base,
    names,
    destDir,
    password: password ?? null,
  })).catch(error => failed(id, "extract", error));
}

/** Packs sources into a new ZIP archive (fire-and-forget). */
export function startPack(id: string, sources: string[], destZip: string): void {
  if (!hasTauri) {
    demo.demoTransfer("pack", id, sources, destZip, false);
    return;
  }
  void ensureTauriEvents().then(() => invoke<void>("create_archive", { id, sources, destZip })).catch(error => failed(id, "pack", error));
}

/** Cancels a running transfer. */
export function cancelTransfer(id: string): void {
  if (!hasTauri) {
    demo.demoCancelled.add(id);
    return;
  }
  void invoke<void>("cancel_transfer", { id });
}

/** Answers a name conflict. */
export function resolveCollision(
  reqId: string,
  action: "overwrite" | "rename" | "skip",
  applyAll: boolean,
): void {
  if (!hasTauri) {
    const resume = demo.demoCollisionResume.get(reqId);
    demo.demoCollisionResume.delete(reqId);
    resume?.();
    return;
  }
  void invoke<void>("resolve_collision", { reqId, action, applyAll });
}


function failed(id: string, op: string, error: unknown): void {
  const done = { id, op, ok: 0, errors: [{ item: "", error: ipcError(error) }], cancelled: false, mode: "" };
  doneSubs.forEach(subscriber => subscriber(done));
}
