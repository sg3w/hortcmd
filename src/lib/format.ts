// Formatting of sizes, dates, and file names.

import type { DirEntry } from "@/ipc/bindings";
import type { DateFormat, SizeFormat } from "@/store/settingsStore";

const GROUP = new Intl.NumberFormat("de-DE");

export function formatSize(
  bytes: number | bigint,
  format: SizeFormat = "auto",
): string {
  const n = typeof bytes === "bigint" ? Number(bytes) : bytes;
  if (format === "bytes") return `${GROUP.format(n)} B`;
  if (n < 1024) return `${n} B`;
  const units = ["K", "M", "G", "T"];
  let v = n / 1024;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[u]}`;
}

export function formatDate(
  epochSecs: number | bigint,
  format: DateFormat = "medium",
): string {
  const secs = typeof epochSecs === "bigint" ? Number(epochSecs) : epochSecs;
  if (!secs) return "";
  const d = new Date(secs * 1000);
  const p = (x: number) => String(x).padStart(2, "0");
  const time = `${p(d.getHours())}:${p(d.getMinutes())}`;
  switch (format) {
    case "iso":
      return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${time}`;
    case "short":
      return `${p(d.getDate())}.${p(d.getMonth() + 1)}.${String(
        d.getFullYear(),
      ).slice(-2)} ${time}`;
    default:
      return `${p(d.getDate())}.${p(d.getMonth() + 1)}.${d.getFullYear()} ${time}`;
  }
}

/**
 * Unix permission bits (st_mode) as the string "drwxr-xr-x".
 * The first character from is_dir/is_symlink, the remaining nine from the lower
 * bits. Empty string when no permissions are known (e.g. Windows/archive).
 */
export function formatMode(
  mode: number | null,
  isDir: boolean,
  isSymlink: boolean,
): string {
  if (mode == null) return "";
  const type = isSymlink ? "l" : isDir ? "d" : "-";
  const rwx = (bits: number) =>
    (bits & 4 ? "r" : "-") + (bits & 2 ? "w" : "-") + (bits & 1 ? "x" : "-");
  return (
    type + rwx((mode >> 6) & 7) + rwx((mode >> 3) & 7) + rwx(mode & 7)
  );
}

/** Splits a file name into base name and extension. */
export function splitName(entry: Pick<DirEntry, "name" | "is_dir">): {
  base: string;
  ext: string;
} {
  if (entry.is_dir) return { base: entry.name, ext: "" };
  const dot = entry.name.lastIndexOf(".");
  if (dot <= 0) return { base: entry.name, ext: "" };
  return { base: entry.name.slice(0, dot), ext: entry.name.slice(dot + 1) };
}
