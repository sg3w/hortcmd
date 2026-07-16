// ============================================================
// Schnellfilter-Leiste: erscheint, sobald für den aktiven Tab ein
// Filter getippt wurde. Zeigt Filtertext + Trefferzahl und lässt
// sich per X (oder Esc in der Liste) löschen.
// ============================================================

import { Filter, X } from "lucide-react";
import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useT } from "@/store/settingsStore";

export function FilterBar({ side }: { side: Side }) {
  const filter = usePanes((s) => panelOf(s, side).filter);
  const count = usePanes(
    (s) => panelOf(s, side).entries.filter((e) => !e.parent).length,
  );
  const setFilter = usePanes((s) => s.setFilter);
  const t = useT();

  if (!filter) return null;

  return (
    <div className="flex flex-shrink-0 items-center gap-2 border-t border-edge bg-accent-dim px-2 py-1 text-[12px]">
      <Filter size={13} className="shrink-0 text-accent" />
      <span className="shrink-0 text-dim">{t("filter.label")}</span>
      <span className="min-w-0 flex-1 truncate font-mono text-text">{filter}</span>
      <span className="shrink-0 text-dim">
        {count} {t("filter.matches")}
      </span>
      <button
        onClick={() => setFilter(side, "")}
        title={t("filter.clear")}
        className="shrink-0 rounded p-0.5 text-dim hover:bg-header hover:text-text"
      >
        <X size={13} />
      </button>
    </div>
  );
}
