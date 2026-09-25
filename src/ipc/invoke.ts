// ============================================================
// The IPC transport (TICKET-035).
//
// One place decides whether a Tauri runtime is present and how a
// command reaches it. Every domain client below imports `invoke` from
// here and nothing else knows about `@tauri-apps/api`.
//
// The core module is imported lazily on the first call: a top-level
// await would not fit the build target, and the browser demo must never
// load it at all.
// ============================================================

import { ipcError } from "@/lib/appError";

/** Runtime mode is read at use time, including after test harness switches. */
export function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>, options?: { headers: Record<string, string> }) => Promise<T>;

let invokePromise: Promise<InvokeFn> | null = null;

function getInvoke(): Promise<InvokeFn> {
  if (!invokePromise) {
    invokePromise = import("@tauri-apps/api/core").then(
      (mod) => mod.invoke as InvokeFn,
    );
  }
  return invokePromise;
}

export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  // Native-only domain calls share this refusal before loading Tauri. Domain
  // functions with explicit demo fallbacks return before reaching this point.
  if (!hasTauri()) {
    throw {
      ...ipcError(null), code: "capability.unsupported", params: { scheme: "browser" },
      op: cmd, retryable: false, action: null,
    };
  }
  const fn = await getInvoke();
  if (!/^(copy_entries|move_entries|delete_entries|rename_entry|rename_batch|make_dir|net_connect|net_disconnect|net_cancel_connect|cancel_transfer|pause_transfer|pack_archive|extract_archive|plugin_open|plugin_close)$/.test(cmd)) return fn<T>(cmd, args);
  let correlation: string;
  try { correlation = crypto.randomUUID().replaceAll("-", ""); } catch { return fn<T>(cmd, args); }
  const report = (event: "started" | "finished" | "failed", code?: string) => {
    try { void fn("diagnostics_event", { correlation, event, code: code ?? null }).catch(() => {}); } catch { /* Diagnostics cannot interrupt an operation. */ }
  };
  report("started");
  try {
    const result = await fn<T>(cmd, args, { headers: { "x-hortcmd-correlation": correlation } });
    report("finished");
    return result;
  } catch (error) {
    report("failed", typeof error === "object" && error !== null && "code" in error && typeof error.code === "string" ? error.code : undefined);
    throw error;
  }
}
