import { usePlugins } from "./pluginsStore";
import { publishContext } from "./context";
import { pluginOpen } from "@/ipc/domains/plugins";
import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { joinPath } from "@/lib/path";
import { guard } from "@/features/commander/guard";
import { notify } from "@/store/noticeStore";
export const STORAGE_PLUGIN = "org.hortcmd.storage-usage";
export function storageUsageAvailable() {
  return usePlugins.getState().records.some(p => p.id === STORAGE_PLUGIN && p.active && !p.error &&
    ["read:context", "read:fs", "ui:window", "ui:commandbar"].every(permission => p.granted.includes(permission as typeof p.granted[number])));
}
export async function launchStorageUsage(path?: string, side: Side = usePanes.getState().active) {
  if (!storageUsageAvailable()) return;
  const pane = panelOf(usePanes.getState(), side);
  if (pane.archive) return;
  const entry = pane.entries[pane.cursor];
  const root = path ?? (entry?.is_dir && !entry.parent ? joinPath(pane.path, entry.name) : pane.path);
  if (!guard(side, root, "recursiveSize")) return;
  try { await publishContext(); await pluginOpen(STORAGE_PLUGIN, null, true, side, root); }
  catch { notify(side, "storageUsage.failed", {}, "error"); }
}
