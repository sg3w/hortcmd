// ============================================================
// Ordnet einem Eintrag ein Lucide-Icon (+ Farbe) zu.
// ============================================================

import {
  CornerLeftUp,
  File,
  FileArchive,
  FileCode,
  FileImage,
  FileText,
  Folder,
  type LucideIcon,
} from "lucide-react";
import type { Row } from "@/store/panesStore";
import { splitName } from "@/lib/format";

const ARCHIVE = new Set(["zip", "rar", "7z", "tar", "gz", "bz2", "xz"]);
const IMAGE = new Set(["jpg", "jpeg", "png", "gif", "svg", "webp", "bmp", "ico", "heic"]);
const CODE = new Set([
  "js", "ts", "tsx", "jsx", "rs", "py", "sh", "json", "html", "css",
  "c", "cpp", "h", "go", "java", "rb", "php", "toml", "yaml", "yml",
]);
const TEXT = new Set(["txt", "md", "pdf", "doc", "docx", "rtf", "log", "csv"]);

interface IconSpec {
  Icon: LucideIcon;
  className: string;
}

export function iconFor(row: Row): IconSpec {
  if (row.parent) return { Icon: CornerLeftUp, className: "text-dim" };
  if (row.is_dir) return { Icon: Folder, className: "text-accent" };

  const ext = splitName(row).ext.toLowerCase();
  if (ARCHIVE.has(ext)) return { Icon: FileArchive, className: "text-amber-400" };
  if (IMAGE.has(ext)) return { Icon: FileImage, className: "text-purple-400" };
  if (CODE.has(ext)) return { Icon: FileCode, className: "text-emerald-400" };
  if (TEXT.has(ext)) return { Icon: FileText, className: "text-sky-300" };
  return { Icon: File, className: "text-dim" };
}

/** Fertig gerendertes Zeilen-Icon; Größe für Listen-/Miniaturansicht anpassbar. */
export function RowIcon({ row, size = 14 }: { row: Row; size?: number }) {
  const { Icon, className } = iconFor(row);
  return (
    <Icon size={size} className={`inline-block shrink-0 ${className}`} aria-hidden />
  );
}
