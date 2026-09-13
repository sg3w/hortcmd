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

  const focusActive = () => queueMicrotask(() => {
    const state = usePanes.getState()[side];
    document.getElementById(`${side}-tab-${state.tabs[state.activeTab].id}`)?.focus();
  });
  const close = (index: number) => { closeTab(side, index); focusActive(); };

  const scrollStrip = (delta: number) =>
    stripRef.current?.scrollBy({
      left: delta,
      behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth",
    });

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

      {/* Keep close buttons outside the tablist's accessibility ownership. */}
      <div role="tablist" aria-label={t(side === "left" ? "tab.left" : "tab.right")}
        aria-owns={tabs.map(tab => `${side}-tab-${tab.id}`).join(" ")} />
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
              key={tab.id}
              data-active={isActive}
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
              <button
                id={`${side}-tab-${tab.id}`}
                role="tab"
                aria-selected={isActive}
                aria-controls={`${side}-files`}
                aria-keyshortcuts="Delete"
                tabIndex={isActive ? 0 : -1}
                onClick={() => selectTab(side, i)}
                onKeyDown={event => {
                  let next = i;
                  switch (event.key) {
                    case "ArrowLeft": next = (i + tabs.length - 1) % tabs.length; break;
                    case "ArrowRight": next = (i + 1) % tabs.length; break;
                    case "Home": next = 0; break;
                    case "End": next = tabs.length - 1; break;
                    case "Delete": event.preventDefault(); event.stopPropagation(); close(i); return;
                    case "Enter": case " ": break;
                    default: return;
                  }
                  event.preventDefault(); event.stopPropagation();
                  selectTab(side, next); focusActive();
                }}
                className="flex min-w-0 items-center gap-1"
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
              </button>
              {tabs.length > 1 && (
                <button
                  aria-label={`${t("tab.close")}: ${label}`}
                  tabIndex={-1}
                  onClick={() => close(i)}
                  className={cn(
                    "rounded p-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
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
        onClick={() => void addTab(side).then(focusActive)}
        className="flex shrink-0 items-center rounded px-1.5 text-dim hover:bg-accent-dim hover:text-text"
      >
        <Plus size={13} />
      </button>
    </div>
  );
}
