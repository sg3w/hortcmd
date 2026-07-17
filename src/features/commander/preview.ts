// ============================================================
// File preview (F3): reads the cursor entry and opens the
// preview dialog. Folders and ".." are skipped.
// ============================================================

import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useOps } from "@/store/opsStore";
import { readPreview } from "@/ipc/client";
import { joinPath } from "@/lib/path";
import { translate } from "@/store/settingsStore";

const MAX_PREVIEW_BYTES = 256 * 1024;

export async function runView(side: Side): Promise<void> {
  const p = panelOf(usePanes.getState(), side);
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent || cur.is_dir) return;

  const full = joinPath(p.path, cur.name);
  try {
    const preview = await readPreview(full, MAX_PREVIEW_BYTES);
    useOps.getState().setPreview(preview);
  } catch (e) {
    useOps.getState().requestConfirm({
      title: translate("op.errors"),
      message: String(e),
      onConfirm: () => {},
    });
  }
}
