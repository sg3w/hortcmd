import { create } from "zustand";
import { useEffect } from "react";
import type { Drive } from "@/ipc/bindings";
import { ensureTauriEvents, listDrives, onDrivesChanged } from "@/ipc/client";
import { parseLocation } from "@/lib/location";

export const useDriveState = create<{ drives: Drive[]; removed: Drive[] }>(() => ({ drives: [], removed: [] }));
let users = 0;
let generation = 0;
let stop: (() => void) | undefined;

function apply(drives: Drive[]) {
  if (!drives.length) return; // failed/empty metadata reads are not an unmount.
  const unique = [...new Map(drives.map(d => [d.mount, d])).values()];
  useDriveState.setState(s => ({ drives: unique, removed: [...new Map([...s.removed, ...s.drives].filter(d => !unique.some(n => n.mount === d.mount)).map(d => [d.mount, d])).values()].slice(-256) }));
}
export function acquireDrives(): () => void {
  if (users++ === 0) {
    const token = ++generation;
    let events = 0;
    stop = onDrivesChanged(drives => { if (token === generation) { events++; apply(drives); } });
    void ensureTauriEvents().then(async () => {
      if (token !== generation) return;
      const version = events;
      const initial = await listDrives();
      if (token === generation && events === version) apply(initial);
    }).catch(() => { /* retain the last known list; subsequent events recover */ });
  }
  let released = false;
  return () => { if (released) return; released = true; if (--users === 0) { generation++; stop?.(); stop = undefined; } };
}
export function useDrives(): Drive[] {
  useEffect(acquireDrives, []);
  return useDriveState(s => s.drives);
}
/** Presentation membership, with component boundaries and no remote/local mixing. */
export function onMount(path: string, mount: string): boolean {
  try {
    const location = parseLocation(path).location;
    if (location.kind !== "local") return false;
    const raw = location.path.raw;
    const separator = location.path.flavor === "windows" ? "\\" : "/";
    return raw === mount || raw.startsWith(mount.endsWith(separator) ? mount : mount + separator);
  } catch { return false; }
}
