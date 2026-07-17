// ============================================================
// Directory tree next to the file list (view mode "tree").
// Rooted at the drive/home root and automatically expanded
// down to the current path. Subfolders are lazily loaded via
// list_dir. Single click selects, double click
// (or Enter) loads the folder into the list.
// ============================================================

import { useEffect, useReducer, useRef, useState, type KeyboardEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronRight, Folder, FolderOpen } from "lucide-react";
import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { listDir } from "@/ipc/client";
import { useSettings } from "@/store/settingsStore";
import { ancestorChain, baseName, joinPath, rootOf } from "@/lib/path";
import { cn } from "@/lib/cn";

const NODE_HEIGHT = 22;
const INDENT = 12;
const COLLATOR = new Intl.Collator("de", { numeric: true, sensitivity: "base" });

interface TreeNode {
  path: string;
  depth: number;
}

export function DirTree({ side }: { side: Side }) {
  const path = usePanes((s) => panelOf(s, side).path);
  const active = usePanes((s) => s.active === side);
  const loadDir = usePanes((s) => s.loadDir);
  const setActive = usePanes((s) => s.setActive);

  // Tree state in refs (stable across asynchronous loads); `force`
  // triggers the re-render.
  const childrenRef = useRef<Map<string, string[]>>(new Map());
  const expandedRef = useRef<Set<string>>(new Set());
  const loadingRef = useRef<Set<string>>(new Set());
  const [, force] = useReducer((x: number) => x + 1, 0);
  const [selected, setSelected] = useState(path);
  const scrollRef = useRef<HTMLDivElement>(null);

  /** Load the subfolders of a path (cached). */
  async function loadChildren(p: string): Promise<void> {
    if (childrenRef.current.has(p) || loadingRef.current.has(p)) return;
    loadingRef.current.add(p);
    try {
      const res = await listDir(p);
      const hideSystem = useSettings.getState().hideSystemFiles;
      const dirs = res.entries
        .filter((e) => e.is_dir && (!hideSystem || !e.name.startsWith(".")))
        .map((e) => joinPath(res.path, e.name))
        .sort((a, b) => COLLATOR.compare(baseName(a), baseName(b)));
      childrenRef.current.set(p, dirs);
    } catch {
      childrenRef.current.set(p, []);
    } finally {
      loadingRef.current.delete(p);
      force();
    }
  }

  async function toggle(p: string) {
    if (expandedRef.current.has(p)) expandedRef.current.delete(p);
    else {
      expandedRef.current.add(p);
      await loadChildren(p);
    }
    force();
  }

  // Auto-expand down to the current path whenever it changes.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      for (const node of ancestorChain(path)) {
        expandedRef.current.add(node);
        await loadChildren(node);
        if (cancelled) return;
      }
      setSelected(path);
      force();
    })();
    return () => {
      cancelled = true;
    };
  }, [path]);

  // Flatten the visible nodes (only expanded branches).
  const nodes: TreeNode[] = [];
  const walk = (p: string, depth: number) => {
    nodes.push({ path: p, depth });
    if (expandedRef.current.has(p)) {
      for (const k of childrenRef.current.get(p) ?? []) walk(k, depth + 1);
    }
  };
  walk(rootOf(path), 0);

  const virtualizer = useVirtualizer({
    count: nodes.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => NODE_HEIGHT,
    overscan: 12,
  });

  // Keep the selected node in view.
  const selIndex = nodes.findIndex((n) => n.path === selected);
  useEffect(() => {
    if (selIndex >= 0) virtualizer.scrollToIndex(selIndex, { align: "auto" });
  }, [selIndex, virtualizer]);

  const openInList = (p: string) => {
    setActive(side);
    void loadDir(side, p);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const idx = selIndex;
    if (idx < 0) return;
    const move = (to: number) => {
      const n = nodes[to];
      if (n) setSelected(n.path);
    };
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        e.stopPropagation();
        move(idx + 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        e.stopPropagation();
        move(idx - 1);
        break;
      case "ArrowRight":
        e.preventDefault();
        e.stopPropagation();
        if (!expandedRef.current.has(selected)) void toggle(selected);
        else move(idx + 1);
        break;
      case "ArrowLeft":
        e.preventDefault();
        e.stopPropagation();
        if (expandedRef.current.has(selected)) void toggle(selected);
        break;
      case "Enter":
        e.preventDefault();
        e.stopPropagation();
        openInList(selected);
        break;
    }
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col bg-panel">
      <div
        ref={scrollRef}
        tabIndex={0}
        onMouseDown={() => setActive(side)}
        onKeyDown={onKeyDown}
        className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden py-0.5 outline-none"
      >
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((v) => {
            const node = nodes[v.index];
            const kids = childrenRef.current.get(node.path);
            const isExpanded = expandedRef.current.has(node.path);
            // Show the chevron while not loaded (possible subfolders) or
            // when subfolders actually exist.
            const showChevron = kids === undefined || kids.length > 0;
            const isSelected = node.path === selected;
            const label = baseName(node.path);
            return (
              <div
                key={node.path}
                onMouseDown={() => setSelected(node.path)}
                onDoubleClick={() => openInList(node.path)}
                title={node.path}
                className={cn(
                  "absolute left-0 right-0 flex cursor-default items-center gap-1 truncate px-1 text-[12px] leading-[22px] hover:bg-row-alt",
                  isSelected &&
                    (active
                      ? "!bg-accent-dim text-accent outline outline-1 -outline-offset-1 outline-cursor"
                      : "!bg-accent-dim/60"),
                )}
                style={{
                  height: NODE_HEIGHT,
                  transform: `translateY(${v.start}px)`,
                  paddingLeft: node.depth * INDENT + 2,
                }}
              >
                <button
                  onMouseDown={(e) => {
                    e.stopPropagation();
                    e.preventDefault();
                  }}
                  onClick={(e) => {
                    e.stopPropagation();
                    void toggle(node.path);
                  }}
                  className={cn(
                    "flex h-4 w-4 shrink-0 items-center justify-center text-dim",
                    !showChevron && "invisible",
                  )}
                  aria-label={isExpanded ? "Zuklappen" : "Aufklappen"}
                >
                  <ChevronRight
                    size={12}
                    className={cn("transition-transform", isExpanded && "rotate-90")}
                  />
                </button>
                {isExpanded ? (
                  <FolderOpen size={13} className="shrink-0 text-accent" aria-hidden />
                ) : (
                  <Folder size={13} className="shrink-0 text-accent" aria-hidden />
                )}
                <span className="truncate">{label}</span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
