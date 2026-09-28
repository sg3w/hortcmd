import { useId, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { DiffEntry } from "@/ipc/bindings";
import { useSettings, useT } from "@/store/settingsStore";
import { formatDate, formatSize } from "@/lib/format";
import { consumeDialogKey } from "@/lib/keys";
import { cn } from "@/lib/cn";
import type { TransKey } from "@/i18n/dictionaries";

const columns = "28px minmax(180px,1fr) 85px 145px 54px 145px 85px minmax(180px,1fr)";
const statuses: Record<string, [string, TransKey]> = {
  same: ["=", "compare.status.same"], different: ["≠", "compare.status.different"],
  left_only: ["→", "compare.status.left_only"], right_only: ["←", "compare.status.right_only"],
  newer_left: ["›", "compare.status.newer_left"], newer_right: ["‹", "compare.status.newer_right"],
  // A level the walk could not open: reported so the gap is visible, with
  // neither side present, so it offers no copy direction (ticket 384).
  unreadable: ["!", "compare.status.unreadable"],
};
export function CompareResults({entries, selected, toggle, toggleAll, run, left, right}: {
  left:string; right:string; entries: DiffEntry[]; selected: Set<string>; toggle: (rel:string)=>void; toggleAll:()=>void; run:number;
}) {
  const t=useT(); const id=useId(); const scroll=useRef<HTMLDivElement>(null);
  const [cursor,setCursor]=useState(0);
  const size=useSettings(s=>s.sizeFormat); const date=useSettings(s=>s.dateFormat);
  const virtual=useVirtualizer({count:entries.length,getScrollElement:()=>scroll.current,
    estimateSize:()=>28,overscan:12,getItemKey:i=>`${run}:${entries[i].rel}`});
  const items=virtual.getVirtualItems();
  const active=Math.min(cursor,entries.length-1);
  const labels=["compare.col.path","compare.col.size","compare.col.date"] as const;
  return <div ref={scroll} role="grid" aria-label={t("compare.title")} aria-rowcount={entries.length+2}
    aria-colcount={8} tabIndex={0} aria-activedescendant={items.some(v=>v.index===active)?`${id}-${active}`:undefined}
    className="min-h-[160px] flex-1 overflow-auto rounded border border-edge bg-panel"
    onKeyDown={e=>{
      if(e.target!==e.currentTarget) return;
      let next=active;
      if(e.key==="ArrowDown") next++; else if(e.key==="ArrowUp") next--;
      else if(e.key==="Home") next=0; else if(e.key==="End") next=entries.length-1;
      else if(e.key===" "){consumeDialogKey(e);if(entries[active])toggle(entries[active].rel);return;}
      else return;
      consumeDialogKey(e); next=Math.max(0,Math.min(entries.length-1,next));setCursor(next);virtual.scrollToIndex(next);
    }}>
    <div className="min-w-[1002px]">
      <div role="row" aria-rowindex={1} className="sticky top-0 z-20 grid items-center border-b border-edge bg-header text-[12px] text-text" style={{gridTemplateColumns:columns,height:36}}>
        <div role="columnheader"/>
        <div role="columnheader" aria-colspan={3} style={{gridColumn:"span 3"}} className="flex min-w-0 items-center gap-2 px-2"><span>{t("compare.left")}</span><input readOnly aria-label={t("compare.left")} title={left} value={left} className="min-w-0 flex-1 bg-transparent font-mono"/></div>
        <div role="columnheader"/>
        <div role="columnheader" aria-colspan={3} style={{gridColumn:"span 3"}} className="flex min-w-0 items-center gap-2 px-2"><span>{t("compare.right")}</span><input readOnly aria-label={t("compare.right")} title={right} value={right} className="min-w-0 flex-1 bg-transparent font-mono"/></div>
      </div>
      <div role="row" aria-rowindex={2} className="sticky top-[36px] z-10 grid items-center border-b border-edge bg-header text-[11px] text-dim" style={{gridTemplateColumns:columns,height:28}}>
        <div role="columnheader" className="px-1"><input type="checkbox" aria-label={t("compare.selectAll")}
          checked={entries.length>0&&entries.every(e=>selected.has(e.rel))} disabled={!entries.length} onChange={toggleAll}/></div>
        {labels.map(k=><div role="columnheader" key={k} aria-label={`${t("compare.left")} · ${t(k)}`} className="truncate px-2">{t(k)}</div>)}
        <div role="columnheader" className="text-center">{t("compare.col.status")}</div>
        {[...labels].reverse().map(k=><div role="columnheader" key={k} aria-label={`${t("compare.right")} · ${t(k)}`} className="truncate px-2">{t(k)}</div>)}
      </div>
      <div role="rowgroup" className="relative" style={{height:virtual.getTotalSize()}}>
        {items.map(v=>{
          const e=entries[v.index];
          const unknown=(e.left&&e.left_modified===0)||(e.right&&e.right_modified===0)||!statuses[e.status];
          const [symbol,key]=unknown?["?","compare.status.unknown" as TransKey]:statuses[e.status];
          const values=[e.left?e.rel:"—",e.left?formatSize(e.left_size,size):"—",e.left&&e.left_modified?formatDate(e.left_modified,date):"—",
            e.right&&e.right_modified?formatDate(e.right_modified,date):"—",e.right?formatSize(e.right_size,size):"—",e.right?e.rel:"—"];
          const cell=(value:string,i:number)=><div role="gridcell" key={i} title={value} className={cn("truncate px-2",(i===1||i===4)&&"text-right")}>{value}</div>;
          return <div role="row" id={`${id}-${v.index}`} key={v.key} aria-rowindex={v.index+3} aria-selected={selected.has(e.rel)}
            className={cn("absolute inset-x-0 grid items-center border-b border-edge/40 font-mono text-[11px] text-text",selected.has(e.rel)&&"bg-selection text-selection-text",active===v.index&&"outline outline-1 -outline-offset-1 outline-accent")}
            style={{gridTemplateColumns:columns,height:28,transform:`translateY(${v.start}px)`}}>
            <div role="gridcell" className="px-1"><input type="checkbox" tabIndex={-1} aria-label={e.rel} checked={selected.has(e.rel)} onChange={()=>toggle(e.rel)}/></div>
            {values.slice(0,3).map(cell)}
            <div role="gridcell" title={t(key)} aria-label={t(key)} className="text-center font-bold">{symbol}</div>
            {values.slice(3).map((value,i)=>cell(value,i+3))}
          </div>;
        })}
      </div>
    </div>
  </div>;
}
