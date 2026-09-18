import { pluginWorkspace } from "@/ipc/domains/plugins";
import type { PluginWorkspace } from "@/ipc/bindings/PluginWorkspace";
import type { PluginCopyOperation } from "@/ipc/bindings/PluginCopyOperation";
import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useSettings } from "@/store/settingsStore";
import { joinPath, parentPath } from "@/lib/path";

export function snapshot(copyOperation: PluginCopyOperation | null = null): PluginWorkspace | null {
  const state = usePanes.getState();
  const copyLimit = Math.max(0, Math.min(0xffffffff, Math.round(useSettings.getState().speedLimit)));
  const pane = (side: Side) => {
    const p = panelOf(state, side); const cursor = p.entries[p.cursor];
    // Archive members are not VFS file paths. Expose the container, never invent
    // a local path that could point to an unrelated file outside the archive.
    if (p.archive) return {side, path:p.archive, folderPath:parentPath(p.archive), filePath:p.archive, selection:[], copyLimit, copyOperation:side === state.active ? copyOperation : null};
    return {side, path:p.path, folderPath:p.path,
      filePath:cursor && !cursor.parent && !cursor.is_dir ? joinPath(p.path, cursor.name) : null,
      selection:[...p.selected].map(name => joinPath(p.path, name)), copyLimit,
      copyOperation:side === state.active ? copyOperation : null};
  };
  const left = pane("left"), right = pane("right");
  return left.path && right.path ? {active:state.active, left, right} : null;
}
export async function publishContext(copyOperation: PluginCopyOperation | null = null) {
  const workspace = snapshot(copyOperation);
  if (workspace) await pluginWorkspace(workspace);
}
