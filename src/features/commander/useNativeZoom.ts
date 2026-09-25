import { useEffect } from "react";
import { hasTauri, setPageZoom } from "@/ipc/client";
import { isMacOS } from "@/lib/platform";
import { usePanes } from "@/store/panesStore";
import { notify } from "@/store/noticeStore";

type ZoomAction = "in" | "out" | "reset";
let percent = 100;
let pending = Promise.resolve();

/** Keep confirmed page zoom separate from font/icon preferences. Queue key
 * repeats so a slow native call cannot overwrite a newer zoom level. */
export function changePageZoom(action: ZoomAction): Promise<void> {
  if (!hasTauri()) return Promise.resolve();
  pending = pending.then(async () => {
    const next = action === "reset" ? 100
      : Math.max(50, Math.min(300, percent + (action === "in" ? 25 : -25)));
    try {
      await setPageZoom(next / 100);
      percent = next;
    } catch {
      notify(usePanes.getState().active, "zoom.failed", undefined, "error");
    }
  });
  return pending;
}

export function useNativeZoom(): void {
  useEffect(() => {
    if (!hasTauri()) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.isComposing || event.altKey || event.code.startsWith("Numpad")) return;
      if (isMacOS ? !event.metaKey || event.ctrlKey : !event.ctrlKey || event.metaKey) return;
      const action = event.key === "+" || event.key === "=" ? "in"
        : event.key === "-" ? "out" : event.key === "0" ? "reset" : null;
      if (!action) return;
      // Page zoom also works in modal inputs and terminals. Capture it before
      // their local key handlers, without forwarding the key to a remote shell.
      event.preventDefault();
      event.stopImmediatePropagation();
      void changePageZoom(action);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
}
