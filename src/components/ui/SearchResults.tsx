import { useId, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Check } from "lucide-react";
import type { SearchHit } from "@/ipc/bindings";
import { useT } from "@/store/settingsStore";
import { formatSize } from "@/lib/format";
import { parentPath } from "@/lib/path";
import { consumeDialogKey } from "@/lib/keys";
import { cn } from "@/lib/cn";

const ROW_H = 40;

export function SearchResults({ hits, onReveal }: {
  hits: SearchHit[];
  onReveal: (hit: SearchHit) => void;
}) {
  const t = useT();
  const id = useId();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [cursor, setCursor] = useState(0);
  const active = Math.min(cursor, hits.length - 1);
  const virtualizer = useVirtualizer({ count: hits.length,
    getScrollElement: () => scrollRef.current, estimateSize: () => ROW_H, overscan: 15 });
  const items = virtualizer.getVirtualItems();
  return <div ref={scrollRef} role="listbox" tabIndex={0}
    aria-label={t("search.results")}
    aria-activedescendant={items.some(v => v.index === active) ? `${id}-${active}` : undefined}
    className="min-h-0 flex-1 overflow-auto"
    onKeyDown={event => {
      let next = active;
      switch (event.key) {
        case "ArrowDown": next++; break;
        case "ArrowUp": next--; break;
        case "Home": next = 0; break;
        case "End": next = hits.length - 1; break;
        case "PageDown": next += Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? ROW_H) / ROW_H)); break;
        case "PageUp": next -= Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? ROW_H) / ROW_H)); break;
        case "Enter":
          consumeDialogKey(event);
          if (hits[active]) onReveal(hits[active]);
          return;
        default: return;
      }
      consumeDialogKey(event);
      if (!hits.length) return;
      next = Math.max(0, Math.min(hits.length - 1, next));
      setCursor(next);
      virtualizer.scrollToIndex(next, { align: "auto" });
    }}>
    <div role="presentation" className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
      {items.map(v => {
        const hit = hits[v.index];
        return <div key={v.key} id={`${id}-${v.index}`} role="option"
          aria-selected={v.index === active} aria-posinset={v.index + 1} aria-setsize={hits.length}
          onClick={() => { setCursor(v.index); scrollRef.current?.focus(); }}
          onDoubleClick={() => onReveal(hit)} title={hit.path}
          className={cn("absolute left-0 top-0 flex w-full cursor-default flex-col justify-center border-b border-edge/50 px-3 pr-8",
            v.index === active ? "bg-accent-dim outline outline-1 -outline-offset-1 outline-accent" : hit.group % 2 === 1 && "bg-accent-dim/30")}
          style={{ height: ROW_H, transform: `translateY(${v.start}px)` }}>
          <div className="flex items-baseline gap-2">
            <span className="truncate text-[12px] text-text">{hit.name}</span>
            {!hit.is_dir && <span className="shrink-0 font-mono text-[11px] text-dim">{formatSize(hit.size)}</span>}
            {hit.detail && <span className="truncate font-mono text-[11px] text-emerald-400">{hit.detail}</span>}
          </div>
          <span className="truncate text-[10px] text-dim">{parentPath(hit.path)}</span>
          {v.index === active && <Check size={14} aria-hidden className="absolute right-3 text-accent" />}
        </div>;
      })}
    </div>
  </div>;
}
