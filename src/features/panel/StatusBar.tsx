import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { formatSize } from "@/lib/format";
import { useT } from "@/store/settingsStore";

export function StatusBar({ side }: { side: Side }) {
  const entries = usePanes((s) => panelOf(s, side).entries);
  const selected = usePanes((s) => panelOf(s, side).selected);
  const t = useT();

  const files = entries.filter((e) => !e.is_dir && !e.parent);
  const dirs = entries.filter((e) => e.is_dir && !e.parent);
  const selBytes = entries
    .filter((e) => selected.has(e.name) && !e.is_dir)
    .reduce((a, e) => a + e.size, 0);

  return (
    <div className="flex-shrink-0 truncate border-t border-edge bg-header px-2 py-[3px] font-mono text-[11px] text-dim">
      {selected.size} {t("status.of")} {files.length} {t("status.files")}{" "}
      {t("status.marked")} – {formatSize(selBytes)}
      {"  |  "}
      {dirs.length} {t("status.folders")}
    </div>
  );
}
