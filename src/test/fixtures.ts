// ============================================================
// Shared test fixtures (TICKET-033).
//
// Directory entries appear in almost every frontend test. Building them
// here means a new field on `DirEntry` is one edit rather than a dozen,
// and — because the builders are typed against the generated binding —
// a field that appears on the Rust side cannot be forgotten in the
// tests either.
// ============================================================

import type { DirEntry } from "@/ipc/bindings";
import type { Row } from "@/store/panesStore";

/** A plain file, overridable field by field. */
export function file(name = "a.txt", over: Partial<DirEntry> = {}): DirEntry {
  return {
    name,
    is_dir: false,
    is_symlink: false,
    size: 1,
    modified: 0,
    mode: null,
    hidden: false,
    readonly: false,
    executable: false,
    ...over,
  };
}

/** A folder. */
export function dir(name = "sub", over: Partial<DirEntry> = {}): DirEntry {
  return file(name, { is_dir: true, size: 0, ...over });
}

/** A row of the file list (an entry plus the optional ".." marker). */
export function row(name = "a.txt", over: Partial<Row> = {}): Row {
  return { ...file(name), ...over };
}
