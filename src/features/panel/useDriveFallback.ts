import { useEffect } from "react";
import type { Drive } from "@/ipc/bindings";
import { homeDir } from "@/ipc/client";
import { useDriveState, onMount } from "@/store/driveStore";
import { panelOf, usePanes } from "@/store/panesStore";
import { notify } from "@/store/noticeStore";

export function useDriveFallback(drives: Drive[]): void {
  const left = usePanes(s => s.left);
  const right = usePanes(s => s.right);
  useEffect(() => {
    const removed = useDriveState.getState().removed;
    for (const side of ["left", "right"] as const) {
      const tab = panelOf(usePanes.getState(), side);
      if (tab.loading) continue;
      const volume = removed.find(d => onMount(tab.archive ?? tab.path, d.mount));
      if (!volume) continue;
      void homeDir().then(home => {
        const current = panelOf(usePanes.getState(), side);
        if (current.id !== tab.id || current.request !== tab.request || current.path !== tab.path || current.archive !== tab.archive) return;
        if (useDriveState.getState().drives.some(d => d.mount === volume.mount)) return;
        notify(side, "drive.removed", { name: volume.name });
        void usePanes.getState().loadDir(side, home);
      }).catch(() => {});
    }
  }, [drives, left, right]);
}
