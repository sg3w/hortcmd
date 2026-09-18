// ============================================================
// File preview (F3): reads the cursor entry and opens the
// preview dialog. Folders may use a plugin renderer; ".." is skipped.
// ============================================================

import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useOps } from "@/store/opsStore";
import { readPreview } from "@/ipc/client";
import { joinPath } from "@/lib/path";
import { translate } from "@/store/settingsStore";
import { guard } from "./guard";

const MAX_PREVIEW_BYTES = 256 * 1024;

export async function runView(side: Side): Promise<void> {
  const p = panelOf(usePanes.getState(), side);
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent) return;

  if (!guard(side, p.path, "preview")) return;

  const full = joinPath(p.path, cur.name);
  try {
    if (cur.is_dir) {
      useOps.getState().setPreview({kind:"text",name:cur.name,size:0,text:translate("plugins.folderFallback"),hex:null,data_url:null,exif:[],truncated:false,path:full,side,folder:true});
    } else {
      const preview = await readPreview(full, MAX_PREVIEW_BYTES);
      useOps.getState().setPreview({...preview,path:full,side});
    }
  } catch (e) {
    useOps.getState().requestConfirm({
      title: translate("op.errors"),
      message: String(e),
      onConfirm: () => {},
    });
  }
}
