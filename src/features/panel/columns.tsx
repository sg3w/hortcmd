// Column definitions for the file table (TanStack Table, headless).
// The columns depend on the settings and are therefore produced by a
// builder (buildColumns), not as a static constant.

import { createColumnHelper, type ColumnDef } from "@tanstack/react-table";
import type { Row, SortKey } from "@/store/panesStore";
import type { DateFormat, SizeFormat } from "@/store/settingsStore";
import { formatDate, formatMode, formatSize, splitName } from "@/lib/format";
import { RowIcon } from "@/lib/fileIcon";
import { DEFAULT_COLUMN_WIDTHS, translate } from "@/store/settingsStore";

const col = createColumnHelper<Row>();

export interface ColumnOptions {
  showExt: boolean;
  showPerms: boolean;
  sizeFormat: SizeFormat;
  dateFormat: DateFormat;
  iconSize: number;
  /** Drag-adjusted widths (px) per column id. */
  widths: Record<string, number>;
}

/** Width (px) of a fixed column from the settings, or the default value. */
function widthPx(id: string, widths: Record<string, number>): string {
  return `${widths[id] ?? DEFAULT_COLUMN_WIDTHS[id]}px`;
}

/** Builds the active columns along with a matching grid template. */
export function buildColumns(opts: ColumnOptions): {
  columns: ColumnDef<Row, string>[];
  template: string;
} {
  const columns: ColumnDef<Row, string>[] = [];
  // Name fills the rest; the other columns have fixed, adjustable widths.
  const parts: string[] = ["minmax(80px, 1fr)"];

  // When the extension column is off, the name column shows the full name.
  // Deliberately derive the display from row.original (not getValue): TanStack caches
  // accessor values per column id, otherwise the old value would remain on toggling.
  columns.push(
    col.accessor((r) => r.name, {
      id: "name",
      header: "Name",
      cell: (c) => {
        const r = c.row.original;
        const label = opts.showExt ? splitName(r).base : r.name;
        return (
          <span className="flex items-center gap-1.5 truncate">
            <RowIcon row={r} size={opts.iconSize} />
            <span className="truncate">{label}</span>
          </span>
        );
      },
    }) as ColumnDef<Row, string>,
  );

  if (opts.showExt) {
    columns.push(
      col.accessor((r) => splitName(r).ext, {
        id: "ext",
        header: "Endg.",
      }) as ColumnDef<Row, string>,
    );
    parts.push(widthPx("ext", opts.widths));
  }

  if (opts.showPerms) {
    columns.push(
      col.accessor((r) => formatMode(r.mode, r.is_dir, r.is_symlink), {
        id: "perms",
        header: "Rechte",
        cell: (c) => (c.row.original.parent ? "" : c.getValue()),
      }) as ColumnDef<Row, string>,
    );
    parts.push(widthPx("perms", opts.widths));
  }

  columns.push(
    col.accessor((r) => String(r.size), {
      id: "size",
      header: "Größe",
      cell: (c) => {
        const r = c.row.original;
        if (r.parent) return "";
        return r.is_dir
          ? translate("col.dirmark")
          : formatSize(r.size, opts.sizeFormat);
      },
    }) as ColumnDef<Row, string>,
  );
  parts.push(widthPx("size", opts.widths));

  columns.push(
    col.accessor((r) => String(r.modified), {
      id: "date",
      header: "Datum",
      cell: (c) => formatDate(c.row.original.modified, opts.dateFormat),
    }) as ColumnDef<Row, string>,
  );
  parts.push(widthPx("date", opts.widths));

  return { columns, template: parts.join(" ") };
}

/** Only these columns are sortable; the ids correspond to SortKey. */
export const SORTABLE: Record<string, SortKey> = {
  name: "name",
  ext: "ext",
  size: "size",
  date: "date",
};
