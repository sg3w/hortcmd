// ============================================================
// Ein Dateifenster: Tab-Leiste, Speicherort, Pfad, Tabelle,
// Status. Kapselt die Öffnen-Logik (Ordnernavigation / ".." hoch).
// ============================================================

import { useRef, useState, type MouseEvent } from "react";
import type { Drive } from "@/ipc/bindings";
import { usePanes, type Side } from "@/store/panesStore";
import { openEntry } from "@/features/commander/navigate";
import { cn } from "@/lib/cn";
import { TabBar } from "./TabBar";
import { DriveSelect } from "./DriveSelect";
import { PathBar } from "./PathBar";
import { FilterBar } from "./FilterBar";
import { StatusBar } from "./StatusBar";
import { FileTable } from "./FileTable";
import { DirTree } from "./DirTree";
import { FileContextMenu } from "@/components/ui/FileContextMenu";

interface Props {
  side: Side;
  drives: Drive[];
}

export function Panel({ side, drives }: Props) {
  const active = usePanes((s) => s.active === side);
  const showTree = usePanes((s) => s[side].viewMode === "tree");
  const open = (index: number) => openEntry(side, index);
  // Index der zuletzt rechtsgeklickten Zeile (für kontextabhängiges Einfügen).
  const ctxIndexRef = useRef<number | null>(null);
  const [treeWidth, setTreeWidth] = useState(220);

  const startTreeDrag = (e: MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = treeWidth;
    const onMove = (ev: globalThis.MouseEvent) =>
      setTreeWidth(Math.min(480, Math.max(120, startW + ev.clientX - startX)));
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const table = (
    <FileContextMenu side={side} targetRef={ctxIndexRef}>
      <FileTable
        side={side}
        onOpen={open}
        onContext={(i) => {
          ctxIndexRef.current = i;
        }}
      />
    </FileContextMenu>
  );

  return (
    <section
      className={cn(
        "flex min-w-0 flex-1 flex-col bg-panel-inactive",
        active && "bg-panel",
      )}
    >
      <TabBar side={side} />
      <DriveSelect side={side} drives={drives} />
      <PathBar side={side} />
      {showTree ? (
        <div className="flex min-h-0 flex-1">
          <div
            className="flex min-w-0 shrink-0 border-r border-edge"
            style={{ width: treeWidth }}
          >
            <DirTree side={side} />
          </div>
          <div
            onMouseDown={startTreeDrag}
            className="w-1 shrink-0 cursor-col-resize bg-edge hover:bg-accent"
          />
          {table}
        </div>
      ) : (
        table
      )}
      <FilterBar side={side} />
      <StatusBar side={side} />
    </section>
  );
}
