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

export const hasTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

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
  const fn = await getInvoke();
  return fn<T>(cmd, args);
}
