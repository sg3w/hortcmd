import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { formatSize } from "@/lib/format";
import { useT } from "@/store/settingsStore";

export function StatusBar({ side }: { side: Side }) {
  const entries = usePanes((s) => panelOf(s, side).entries);
  const selected = usePanes((s) => panelOf(s, side).selected);
  const t = useT();

  const files = entries.filter((e) => !e.is_dir && !e.parent);
  const dirs = entries.filter((e) => e.is_dir && !e.parent);
  // Counted per kind: folders can be marked too (by hand, or via the
  // pattern dialog's "include folders", TICKET-059), and counting them
  // against the file total would read as "15 of 11 files".
  const selFiles = files.filter((e) => selected.has(e.name));
  const selDirs = dirs.filter((e) => selected.has(e.name)).length;
  const selBytes = selFiles.reduce((a, e) => a + e.size, 0);

  return (
    <div className="flex-shrink-0 truncate border-t border-edge bg-header px-2 py-[3px] font-mono text-[11px] text-dim">
      {selFiles.length} {t("status.of")} {files.length} {t("status.files")}{" "}
      {t("status.marked")} – {formatSize(selBytes)}
      {"  |  "}
      {selDirs > 0 ? `${selDirs} ${t("status.of")} ` : ""}
      {dirs.length} {t("status.folders")}
    </div>
  );
}
