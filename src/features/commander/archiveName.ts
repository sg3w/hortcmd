// ============================================================
// Pack: the archive name proposed in the prompt, computed from the
// entries that are about to be packed (TICKET-390).
//
// Folder wins over file, first in list order wins over the rest, and the
// local date and time keep two packs of the same folder apart.
// ============================================================

import type { DirEntry } from "@/ipc/bindings";
import { splitName } from "@/lib/format";

/** Everything the proposal needs from an entry. */
type Packable = Pick<DirEntry, "name" | "is_dir">;

/** Used when nothing packable is known, and when a name sanitizes to nothing. */
const FALLBACK_BASE = "archiv";

/** Keeps the whole proposal clear of the 255-byte name limit of ext4/APFS. */
const MAX_BASE_LENGTH = 100;

/**
 * Name part of the proposal: the first folder if there is one, otherwise the
 * first file without its extension.
 */
function baseOf(entries: Packable[]): string {
  const chosen = entries.find((e) => e.is_dir) ?? entries[0];
  if (!chosen) return FALLBACK_BASE;
  return chosen.is_dir ? chosen.name : splitName(chosen).base;
}

/**
 * Makes a listing name usable as a file name of its own: the separators and
 * the colon that a name must not carry become `_`, and the trailing dots and
 * spaces that Windows drops are removed rather than silently lost later.
 */
function sanitize(base: string): string {
  const cleaned = base
    .replace(/[/\\:]/g, "_")
    .trim()
    .slice(0, MAX_BASE_LENGTH)
    .replace(/[. ]+$/, "");
  return cleaned || FALLBACK_BASE;
}

/** `YYYY-MM-DD_HH-MM-SS` in local time — sortable, and legal in a name. */
export function timestamp(now: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  const date = [now.getFullYear(), pad(now.getMonth() + 1), pad(now.getDate())];
  const time = [now.getHours(), now.getMinutes(), now.getSeconds()].map(pad);
  return `${date.join("-")}_${time.join("-")}`;
}

/** The `.zip` name proposed for packing `entries`, in list order. */
export function proposedArchiveName(entries: Packable[], now: Date): string {
  return `${sanitize(baseOf(entries))}_${timestamp(now)}.zip`;
}

/**
 * Range the prompt preselects: everything but the extension, so typing
 * replaces name and timestamp while `.zip` survives.
 */
export function stemRange(name: string): [number, number] {
  const dot = name.lastIndexOf(".");
  return [0, dot > 0 ? dot : name.length];
}
