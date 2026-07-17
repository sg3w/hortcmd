// ============================================================
// Tab bar of a pane. The active tab is the working
// and target surface. "+" opens another tab (same path).
// When space gets tight, two scroll buttons appear (instead of a
// scrollbar) to shift the tab bar horizontally.
// ============================================================

import { useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Package, Plus, X } from "lucide-react";
import { usePanes, type Side } from "@/store/panesStore";
import { useT } from "@/store/settingsStore";
import { cn } from "@/lib/cn";

function basename(path: string): string {
  if (!path || path === "/") return "/";
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || "/";
}

export function TabBar({ side }: { side: Side }) {
  const tabs = usePanes((s) => s[side].tabs);
  const activeTab = usePanes((s) => s[side].activeTab);
  const paneActive = usePanes((s) => s.active === side);
  const selectTab = usePanes((s) => s.selectTab);
  const closeTab = usePanes((s) => s.closeTab);
  const addTab = usePanes((s) => s.addTab);
  const t = useT();

  const stripRef = useRef<HTMLDivElement>(null);
  const [canLeft, setCanLeft] = useState(false);
  const [canRight, setCanRight] = useState(false);

  const updateScroll = () => {
    const el = stripRef.current;
    if (!el) return;
    setCanLeft(el.scrollLeft > 1);
    setCanRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 1);
  };

  // Re-evaluate the overflow when the bar is resized.
  useEffect(() => {
    const el = stripRef.current;
    if (!el) return;
    updateScroll();
    const ro = new ResizeObserver(updateScroll);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // After a tab change: re-evaluate the overflow + bring the active tab into view.
  useEffect(() => {
    updateScroll();
    stripRef.current
      ?.querySelector<HTMLElement>('[data-active="true"]')
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [tabs.length, activeTab]);

  const scrollStrip = (delta: number) =>
    stripRef.current?.scrollBy({ left: delta, behavior: "smooth" });

  return (
    <div
      className={cn(
        "flex flex-shrink-0 items-stretch gap-0.5 border-b-2 bg-header px-1 pt-1",
        paneActive ? "border-accent" : "border-edge",
      )}
    >
      {canLeft && (
        <button
          title={t("tab.scrollLeft")}
          onClick={() => scrollStrip(-120)}
          className="flex shrink-0 items-center rounded px-0.5 text-dim hover:bg-accent-dim hover:text-text"
        >
          <ChevronLeft size={14} />
        </button>
      )}

      <div
        ref={stripRef}
        onScroll={updateScroll}
        className="flex min-w-0 flex-1 items-stretch gap-0.5 overflow-x-hidden"
      >
        {tabs.map((tab, i) => {
          const isActive = i === activeTab;
          // In einem Archiv: Paket-Icon + Archivname statt des inneren Pfads.
          const label = tab.archive ? basename(tab.archive) : basename(tab.path);
          return (
            <div
              key={i}
              data-active={isActive}
              onMouseDown={() => selectTab(side, i)}
              title={tab.archive ?? tab.path}
              className={cn(
                "group flex max-w-[160px] shrink-0 cursor-default items-center gap-1 rounded-t border border-b-0 px-2 py-0.5 text-[12px]",
                isActive
                  ? paneActive
                    ? "border-accent bg-accent text-white"
                    : "border-edge bg-panel text-text"
                  : "border-transparent bg-panel-inactive text-dim hover:text-text",
              )}
            >
              {tab.archive && (
                <Package
                  size={11}
                  className={cn(
                    "shrink-0",
                    isActive && paneActive ? "text-white/90" : "text-amber-500",
                  )}
                />
              )}
              <span className="truncate">{label}</span>
              {tabs.length > 1 && (
                <button
                  title={t("tab.close")}
                  onMouseDown={(e) => {
                    e.stopPropagation();
                    closeTab(side, i);
                  }}
                  className={cn(
                    "rounded p-0.5 opacity-0 group-hover:opacity-100",
                    isActive && paneActive
                      ? "text-white/80 hover:bg-white/20"
                      : "text-dim hover:bg-accent-dim hover:text-text",
                  )}
                >
                  <X size={11} />
                </button>
              )}
            </div>
          );
        })}
      </div>

      {canRight && (
        <button
          title={t("tab.scrollRight")}
          onClick={() => scrollStrip(120)}
          className="flex shrink-0 items-center rounded px-0.5 text-dim hover:bg-accent-dim hover:text-text"
        >
          <ChevronRight size={14} />
        </button>
      )}

      <button
        title={t("tab.new")}
        onMouseDown={() => addTab(side)}
        className="flex shrink-0 items-center rounded px-1.5 text-dim hover:bg-accent-dim hover:text-text"
      >
        <Plus size={13} />
      </button>
    </div>
  );
}
