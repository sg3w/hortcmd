// ============================================================
// File table: TanStack Table (headless) + TanStack Virtual.
// Only visible rows are rendered. Cursor/selection come
// from the store and are mapped to CSS classes.
//
// Three view modes share one grid virtualization:
//   details      – one column, full detail columns + header row
//   list         – multi-column, compact (icon + name)
//   thumbnails   – multi-column, large icons with the name below
// ============================================================

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent,
} from "react";
import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type Cell,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronDown, ChevronUp } from "lucide-react";
import { panelOf, usePanes, type Row, type Side } from "@/store/panesStore";
import {
  DEFAULT_COLUMN_WIDTHS,
  useSettings,
  useT,
} from "@/store/settingsStore";
import type { TransKey } from "@/i18n/dictionaries";
import { RowIcon } from "@/lib/fileIcon";
import {
  fileColorRef,
  resolveFileColor,
  useFileColorRules,
  type FileColorDef,
} from "@/lib/fileColors";
import { buildColumns, SORTABLE } from "./columns";
import { cn } from "@/lib/cn";

/** Font and icon size (px) per scaling step. */
const FONT_PX = { sm: 11, md: 12, lg: 14 } as const;
const ICON_PX = { sm: 14, md: 16, lg: 20 } as const;
/** Fixed height of the thumbnail view (px). */
const THUMB_HEIGHT = 96;
/** Minimum width of a tile in the multi-column views (px). */
const CELL_MIN_WIDTH = { list: 200, thumbnails: 104 } as const;

interface Props {
  side: Side;
  onOpen: (index: number) => void;
  /** Right-click: index of the hit row (or null for empty space/header). */
  onContext?: (index: number | null) => void;
}

export function FileTable({ side, onOpen, onContext }: Props) {
  const entries = usePanes((s) => panelOf(s, side).entries);
  const cursor = usePanes((s) => panelOf(s, side).cursor);
  const selected = usePanes((s) => panelOf(s, side).selected);
  const sort = usePanes((s) => panelOf(s, side).sort);
  const git = usePanes((s) => panelOf(s, side).git);
  // In tree mode the list itself renders as the detail view; the tree sits
  // next to it (see Panel).
  const rawMode = usePanes((s) => s[side].viewMode);
  const viewMode = rawMode === "tree" ? "details" : rawMode;
  const active = usePanes((s) => s.active === side);
  const setActive = usePanes((s) => s.setActive);
  const setCursor = usePanes((s) => s.setCursor);
  const toggleMark = usePanes((s) => s.toggleMark);
  const shiftTo = usePanes((s) => s.shiftTo);
  const setSort = usePanes((s) => s.setSort);
  const setGridCols = usePanes((s) => s.setGridCols);
  const t = useT();

  // Display settings (columns + sizes).
  const showExtColumn = useSettings((s) => s.showExtColumn);
  const showPermissions = useSettings((s) => s.showPermissions);
  const sizeFormat = useSettings((s) => s.sizeFormat);
  const dateFormat = useSettings((s) => s.dateFormat);
  const fontScale = useSettings((s) => s.fontScale);
  const iconScale = useSettings((s) => s.iconScale);
  const columnWidths = useSettings((s) => s.columnWidths);
  const setColumnWidth = useSettings((s) => s.setColumnWidth);
  const fontPx = FONT_PX[fontScale];
  const iconPx = ICON_PX[iconScale];
  // Color rules (built-in + user-defined), sorted by priority.
  const colorRules = useFileColorRules();

  const { columns, template } = useMemo(
    () =>
      buildColumns({
        showExt: showExtColumn,
        showPerms: showPermissions,
        sizeFormat,
        dateFormat,
        iconSize: iconPx,
        widths: columnWidths,
      }),
    [
      showExtColumn,
      showPermissions,
      sizeFormat,
      dateFormat,
      iconPx,
      columnWidths,
    ],
  );

  // Adjust the column width by dragging the left edge of a fixed column.
  // The handle follows the cursor; the flexible name column absorbs the
  // difference. The new width is persisted immediately (settings store).
  const startColResize = (e: MouseEvent, id: string) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startW = columnWidths[id] ?? DEFAULT_COLUMN_WIDTHS[id];
    const onMove = (ev: globalThis.MouseEvent) => {
      setColumnWidth(id, startW + (startX - ev.clientX));
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const scrollRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  // Measure the width of the scroll container → column count of the grid views.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    setWidth(el.clientWidth);
    const ro = new ResizeObserver((obs) => setWidth(obs[0].contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const perRow =
    viewMode === "details"
      ? 1
      : Math.max(1, Math.floor(width / CELL_MIN_WIDTH[viewMode]));
  const rowHeight =
    viewMode === "thumbnails"
      ? THUMB_HEIGHT
      : viewMode === "list"
        ? fontPx + 10
        : fontPx + 8;

  // Report the column count to the store (keyboard navigation in the grid).
  useEffect(() => {
    setGridCols(side, perRow);
  }, [side, perRow, setGridCols]);

  // Mouse selection: Cmd/Ctrl = toggle individually, Shift = range, otherwise cursor.
  const onItemMouseDown = (e: MouseEvent, index: number) => {
    setActive(side);
    if (e.metaKey || e.ctrlKey) toggleMark(side, index);
    else if (e.shiftKey) shiftTo(side, index);
    else setCursor(side, index);
  };

  // Right-click: set the active side + cursor to the hit row and
  // report its index (for context-dependent pasting).
  const onContextMenu = (e: MouseEvent) => {
    const el = (e.target as HTMLElement).closest("[data-index]");
    const index = el ? Number((el as HTMLElement).dataset.index) : null;
    setActive(side);
    if (index != null) setCursor(side, index);
    onContext?.(index);
  };

  const table = useReactTable({
    data: entries,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });
  const rows = table.getRowModel().rows;

  const rowCount = Math.ceil(rows.length / perRow);
  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
  });

  // Re-measure on a mode change (height/columns change).
  useEffect(() => {
    virtualizer.measure();
  }, [rowHeight, perRow, virtualizer]);

  // Always keep the cursor in view (grid: row = cursor / columns).
  useEffect(() => {
    virtualizer.scrollToIndex(Math.floor(cursor / perRow), { align: "auto" });
  }, [cursor, perRow, virtualizer]);

  return (
    <div
      className="flex min-h-0 min-w-0 flex-1 flex-col"
      onContextMenu={onContextMenu}
    >
      {/* Spaltenkopf nur in der Detailansicht */}
      {viewMode === "details" && (
        <div
          className="grid flex-shrink-0 border-b border-edge bg-header text-dim"
          style={{ gridTemplateColumns: template }}
        >
          {table.getFlatHeaders().map((header) => {
            const id = header.column.id;
            const sortable = SORTABLE[id];
            const isSorted = sort.key === id;
            const alignRight = id === "size" || id === "date";
            // Fixed columns are adjustable by dragging their left edge.
            const resizable = id in columnWidths;
            return (
              <div
                key={header.id}
                onClick={() => sortable && setSort(side, sortable)}
                className={cn(
                  "relative flex cursor-pointer items-center gap-1 border-r border-edge px-2 py-[3px] last:border-r-0",
                  alignRight && "justify-end",
                )}
              >
                {resizable && (
                  <div
                    onMouseDown={(e) => startColResize(e, id)}
                    onClick={(e) => e.stopPropagation()}
                    onDoubleClick={(e) => e.stopPropagation()}
                    title={t("col.resize")}
                    className="absolute left-0 top-0 z-10 h-full w-1.5 -translate-x-1/2 cursor-col-resize hover:bg-accent"
                  />
                )}
                <span className="truncate">
                  {t(`col.${id}` as TransKey)}
                </span>
                {isSorted &&
                  (sort.asc ? (
                    <ChevronUp size={12} className="shrink-0 text-accent" />
                  ) : (
                    <ChevronDown size={12} className="shrink-0 text-accent" />
                  ))}
              </div>
            );
          })}
        </div>
      )}

      {/* Virtualisierte Zeilenliste */}
      <div
        ref={scrollRef}
        tabIndex={0}
        onMouseDown={() => setActive(side)}
        className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden outline-none"
      >
        <div
          style={{ height: virtualizer.getTotalSize(), position: "relative" }}
        >
          {virtualizer.getVirtualItems().map((vItem) => {
            const start = vItem.index * perRow;
            const rowStyle = {
              height: rowHeight,
              transform: `translateY(${vItem.start}px)`,
            } as const;

            // ----- Detail view: one row with columns -----
            if (viewMode === "details") {
              const row = rows[start];
              return (
                <DetailRow
                  key={row.id}
                  index={start}
                  cursor={cursor}
                  active={active}
                  selected={selected}
                  cells={row.getVisibleCells()}
                  row={row.original}
                  gitStatus={git?.entries[row.original.name]}
                  rules={colorRules}
                  style={rowStyle}
                  template={template}
                  fontSize={fontPx}
                  onMouseDown={onItemMouseDown}
                  onOpen={onOpen}
                />
              );
            }

            // ----- List/thumbnail view: multi-column grid -----
            const items = rows.slice(start, start + perRow);
            return (
              <div
                key={vItem.index}
                className="absolute left-0 grid w-full"
                style={{
                  ...rowStyle,
                  gridTemplateColumns: `repeat(${perRow}, minmax(0, 1fr))`,
                }}
              >
                {items.map((row, k) => (
                  <GridCell
                    key={row.id}
                    index={start + k}
                    cursor={cursor}
                    active={active}
                    selected={selected}
                    row={row.original}
                    gitStatus={git?.entries[row.original.name]}
                    rules={colorRules}
                    thumbnail={viewMode === "thumbnails"}
                    fontSize={fontPx}
                    iconSize={iconPx}
                    onMouseDown={onItemMouseDown}
                    onOpen={onOpen}
                  />
                ))}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

/** Shared cursor/selection classes for an element. */
function stateClass(isCursor: boolean, isSelected: boolean, active: boolean) {
  return cn(
    isSelected && "!bg-selection text-selection-text",
    isCursor &&
      (active
        ? "!bg-accent-dim outline outline-1 -outline-offset-1 outline-cursor"
        : cn(
            // TICKET-008: the cursor row in the inactive panel gets a
            // subtle green fill instead of only a 1px outline, so it
            // stays clearly visible. With an additional selection, the
            // (more dominant) yellow selection color from above remains.
            "outline outline-1 -outline-offset-1 outline-[color:var(--cursor-inactive)]",
            !isSelected && "!bg-[color:var(--cursor-inactive-marker)]",
          )),
  );
}

interface ItemProps {
  index: number;
  cursor: number;
  active: boolean;
  selected: Set<string>;
  row: Row;
  /** Git status of the entry (feeds the color rules). */
  gitStatus?: string;
  /** Color rules, sorted by priority. */
  rules: FileColorDef[];
  style?: CSSProperties;
  onMouseDown: (e: MouseEvent, index: number) => void;
  onOpen: (index: number) => void;
}

/**
 * The color rule for an entry, or undefined when the entry keeps its
 * default color. A selection paints its own colors over the whole row,
 * and ".." is navigation rather than an entry — neither is colored.
 */
function rowColor(
  rules: FileColorDef[],
  row: Row,
  gitStatus: string | undefined,
  isSelected: boolean,
): FileColorDef | undefined {
  if (isSelected || row.parent) return undefined;
  return resolveFileColor(rules, { ...row, gitStatus });
}

/** One detail row with TanStack cells. */
function DetailRow({
  index,
  cursor,
  active,
  selected,
  cells,
  row,
  gitStatus,
  rules,
  style,
  template,
  fontSize,
  onMouseDown,
  onOpen,
}: ItemProps & {
  cells: Cell<Row, unknown>[];
  template: string;
  fontSize: number;
}) {
  const isCursor = index === cursor;
  const isSelected = selected.has(row.name);
  const rule = rowColor(rules, row, gitStatus, isSelected);
  return (
    <div
      data-index={index}
      onMouseDown={(e) => onMouseDown(e, index)}
      onDoubleClick={() => onOpen(index)}
      className={cn(
        "absolute left-0 grid w-full font-mono",
        index % 2 === 1 && "bg-row-alt",
        row.is_dir && !row.parent && "font-semibold",
        !rule && row.is_dir && !row.parent && "text-dir",
        !rule && row.parent && "text-dim",
        rule?.extraClass,
        stateClass(isCursor, isSelected, active),
      )}
      style={{
        ...style,
        gridTemplateColumns: template,
        fontSize,
        lineHeight: `${style?.height ?? fontSize + 8}px`,
        color: rule && fileColorRef(rule.id),
      }}
    >
      {cells.map((cell) => {
        const id = cell.column.id;
        return (
          <div
            key={cell.id}
            className={cn(
              "truncate px-2",
              (id === "size" || id === "date") && "text-right text-dim",
              id === "perms" && "text-dim",
            )}
          >
            {flexRender(cell.column.columnDef.cell, cell.getContext())}
          </div>
        );
      })}
    </div>
  );
}

/** One tile in the list or thumbnail view. */
function GridCell({
  index,
  cursor,
  active,
  selected,
  row,
  gitStatus,
  rules,
  thumbnail,
  fontSize,
  iconSize,
  onMouseDown,
  onOpen,
}: ItemProps & { thumbnail: boolean; fontSize: number; iconSize: number }) {
  const isCursor = index === cursor;
  const isSelected = selected.has(row.name);
  const rule = rowColor(rules, row, gitStatus, isSelected);
  return (
    <div
      data-index={index}
      onMouseDown={(e) => onMouseDown(e, index)}
      onDoubleClick={() => onOpen(index)}
      title={row.name}
      style={{ fontSize, color: rule && fileColorRef(rule.id) }}
      className={cn(
        "flex min-w-0 cursor-default items-center gap-2 rounded px-2 hover:bg-row-alt",
        thumbnail && "h-full flex-col justify-center gap-1 py-1 text-center",
        row.is_dir && !row.parent && "font-semibold",
        !rule && row.is_dir && !row.parent && "text-dir",
        !rule && row.parent && "text-dim",
        rule?.extraClass,
        stateClass(isCursor, isSelected, active),
      )}
    >
      <RowIcon row={row} size={thumbnail ? 40 : iconSize} />
      <span className={cn("truncate", thumbnail && "w-full leading-tight")}>
        {row.name}
      </span>
    </div>
  );
}
