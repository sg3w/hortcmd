// ============================================================
// File-list export: formatting the (displayed) entries as
// text/CSV/TSV/JSON/XML. Pure presentation logic – the actual
// writing to disk is done by the backend (`write_text_file`).
// ============================================================

import type { Row } from "@/store/panesStore";
import type { DateFormat, SizeFormat } from "@/store/settingsStore";
import { formatDate, formatMode, formatSize, splitName } from "@/lib/format";
import { joinPath } from "@/lib/path";

export type ExportFormat = "list" | "csv" | "tsv" | "json" | "xml";
export type ExportField =
  | "path"
  | "name"
  | "ext"
  | "size"
  | "modified"
  | "type"
  | "perms";

/** Canonical field order (independent of the selection order). */
export const EXPORT_FIELDS: ExportField[] = [
  "path",
  "name",
  "ext",
  "size",
  "modified",
  "type",
  "perms",
];

/** File extension per format – for the suggested file name. */
export const FORMAT_EXT: Record<ExportFormat, string> = {
  list: "txt",
  csv: "csv",
  tsv: "tsv",
  json: "json",
  xml: "xml",
};

export interface ExportOptions {
  format: ExportFormat;
  /** Fields to output (at least one), in any order. */
  fields: ExportField[];
  /** Include folders in the output. */
  includeFolders: boolean;
  /** Only selected entries (otherwise the whole list). */
  onlySelected: boolean;
  /** Header row with column names (CSV/TSV only). */
  header: boolean;
  /** Size/date human-readable (otherwise raw values: bytes or ISO-8601). */
  formatValues: boolean;
}

interface Ctx {
  dir: string;
  opts: ExportOptions;
  sizeFormat: SizeFormat;
  dateFormat: DateFormat;
}

/**
 * Picks the rows to export from the displayed ones: without the
 * ".." row, optionally only selected ones, optionally without folders.
 */
export function pickRows(
  rows: Row[],
  selected: Set<string>,
  opts: Pick<ExportOptions, "onlySelected" | "includeFolders">,
): Row[] {
  return rows.filter((r) => {
    if (r.parent) return false;
    if (opts.onlySelected && !selected.has(r.name)) return false;
    if (!opts.includeFolders && r.is_dir) return false;
    return true;
  });
}

/** Value of a field as a string (for text/CSV/TSV/XML). */
function fieldValue(field: ExportField, r: Row, ctx: Ctx): string {
  switch (field) {
    case "path":
      return joinPath(ctx.dir, r.name);
    case "name":
      return r.name;
    case "ext":
      return splitName(r).ext;
    case "size":
      if (r.is_dir) return "";
      return ctx.opts.formatValues
        ? formatSize(r.size, ctx.sizeFormat)
        : String(r.size);
    case "modified":
      if (!r.modified) return "";
      return ctx.opts.formatValues
        ? formatDate(r.modified, ctx.dateFormat)
        : new Date(r.modified * 1000).toISOString();
    case "type":
      return r.is_dir ? "dir" : "file";
    case "perms":
      return formatMode(r.mode, r.is_dir, r.is_symlink);
  }
}

/** JSON-typed value: raw values stay a number or an ISO string. */
function jsonValue(field: ExportField, r: Row, ctx: Ctx): string | number | null {
  if (!ctx.opts.formatValues) {
    if (field === "size") return r.is_dir ? null : r.size;
    if (field === "modified")
      return r.modified ? new Date(r.modified * 1000).toISOString() : null;
  }
  return fieldValue(field, r, ctx);
}

function csvCell(value: string, delim: string): string {
  if (
    value.includes(delim) ||
    value.includes('"') ||
    value.includes("\n") ||
    value.includes("\r")
  ) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

function xmlEscape(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function buildDelimited(rows: Row[], ctx: Ctx, delim: string): string {
  const { fields, header } = ctx.opts;
  const lines: string[] = [];
  if (header) lines.push(fields.map((f) => csvCell(f, delim)).join(delim));
  for (const r of rows) {
    lines.push(
      fields.map((f) => csvCell(fieldValue(f, r, ctx), delim)).join(delim),
    );
  }
  return lines.join("\n");
}

function buildJson(rows: Row[], ctx: Ctx): string {
  const items = rows.map((r) =>
    Object.fromEntries(ctx.opts.fields.map((f) => [f, jsonValue(f, r, ctx)])),
  );
  return JSON.stringify(items, null, 2);
}

function buildXml(rows: Row[], ctx: Ctx): string {
  const lines = ['<?xml version="1.0" encoding="UTF-8"?>', "<files>"];
  for (const r of rows) {
    lines.push("  <file>");
    for (const f of ctx.opts.fields) {
      lines.push(`    <${f}>${xmlEscape(fieldValue(f, r, ctx))}</${f}>`);
    }
    lines.push("  </file>");
  }
  lines.push("</files>");
  return lines.join("\n");
}

/** Builds the export text from the selected rows. */
export function buildExport(
  rows: Row[],
  dir: string,
  opts: ExportOptions,
  formats: { sizeFormat: SizeFormat; dateFormat: DateFormat },
): string {
  const ctx: Ctx = {
    dir,
    opts,
    sizeFormat: formats.sizeFormat,
    dateFormat: formats.dateFormat,
  };
  switch (opts.format) {
    case "csv":
      return buildDelimited(rows, ctx, ",");
    case "tsv":
      return buildDelimited(rows, ctx, "\t");
    case "json":
      return buildJson(rows, ctx);
    case "xml":
      return buildXml(rows, ctx);
    case "list":
      // One line per entry; multiple fields separated by a tab.
      return rows
        .map((r) => opts.fields.map((f) => fieldValue(f, r, ctx)).join("\t"))
        .join("\n");
  }
}
