// ============================================================
// File-system commands (TICKET-035).
//
// Listing, creating, renaming, deleting, previewing and handing a path
// to the operating system. Every one is a thin typed call; the rules
// about *whether* a location may do a thing live in the capability
// contract on the Rust side (ticket 031).
// ============================================================

import type {
  DirListing,
  Drive,
  OpResult,
  Preview,
} from "../bindings";
import { hasTauri, invoke } from "../invoke";
import * as demo from "../demo";
import { gitReadySubs } from "../events";

export async function listDir(path: string): Promise<DirListing> {
  if (!hasTauri) return demo.demoListDir(path);
  return invoke<DirListing>("list_dir", { path });
}

export async function listDrives(): Promise<Drive[]> {
  if (!hasTauri) return demo.demoDrives();
  return invoke<Drive[]>("list_drives");
}

export async function homeDir(): Promise<string> {
  if (!hasTauri) return "/Users/demo";
  return invoke<string>("home_dir");
}

export async function makeDir(path: string): Promise<string> {
  if (!hasTauri) return path;
  return invoke<string>("make_dir", { path });
}

/**
 * Writes text into a file (e.g. an exported file list).
 * Without `overwrite`, an existing file is not overwritten; the
 * return value `false` indicates the name conflict (`true` = written).
 */
export async function writeTextFile(
  path: string,
  contents: string,
  overwrite: boolean,
): Promise<boolean> {
  if (!hasTauri) {
    console.log("[demo] write_text_file", path, `${contents.length} B`, { overwrite });
    return true;
  }
  return invoke<boolean>("write_text_file", { path, contents, overwrite });
}

/** Renames an entry (full source and target path). */
export async function renameEntry(from: string, to: string): Promise<void> {
  if (!hasTauri) {
    console.log("[demo] rename_entry", from, to);
    return;
  }
  return invoke<void>("rename_entry", { from, to });
}

/**
 * Renames several entries in `dir` (batch rename). The target names are
 * computed by the frontend; the backend executes them safely (two-phase,
 * conflict check) and returns the success count + error list.
 */
export async function renameBatch(
  dir: string,
  renames: [string, string][],
): Promise<OpResult> {
  if (!hasTauri) {
    console.log("[demo] rename_batch", dir, `${renames.length} Einträge`);
    return { ok: renames.length, errors: [] };
  }
  return invoke<OpResult>("rename_batch", { dir, renames });
}

export async function deleteEntries(paths: string[]): Promise<OpResult> {
  if (!hasTauri) return { ok: paths.length, errors: [] };
  return invoke<OpResult>("delete_entries", { paths });
}

/** Moves entries to the system trash (restorable). */
export async function trashEntries(paths: string[]): Promise<OpResult> {
  if (!hasTauri) {
    console.log("[demo] trash_entries", paths);
    return { ok: paths.length, errors: [] };
  }
  return invoke<OpResult>("trash_entries", { paths });
}

/** Reads an excerpt of a file for the preview (F3). */
export async function readPreview(
  path: string,
  maxBytes: number,
): Promise<Preview> {
  if (!hasTauri) return demo.demoPreview(path);
  return invoke<Preview>("read_preview", { path, maxBytes });
}

/**
 * Opens a file with the default program (F4). Rejects when the path cannot be
 * opened — a dangling symlink, for instance — so the caller can report it
 * instead of failing silently (TICKET-018).
 */
export async function openPath(path: string): Promise<void> {
  if (!hasTauri) {
    console.log("[demo] open_path", path);
    return;
  }
  return invoke<void>("open_path", { path });
}

/** Opens an `http(s)` link from rendered content in the system browser
 *  (TICKET-030). The backend validates the scheme again and refuses
 *  anything else, so this is never a general launcher. */
export async function openExternal(url: string): Promise<void> {
  if (!hasTauri) {
    console.log("[demo] open_external", url);
    return;
  }
  return invoke<void>("open_external", { url });
}

/** Native macOS Quick Look preview (Space). */
export function quickLook(path: string): void {
  if (!hasTauri) {
    console.log("[demo] quick_look", path);
    return;
  }
  void invoke<void>("quick_look", { path });
}

/**
 * Opens the operating system's own information/properties window for a
 * local entry (Finder "Get Info", the Explorer properties sheet, the
 * desktop file manager's properties dialog). Resolves once the window
 * has been handed to the system; it outlives hortcmd's own dialog.
 */
export async function openSystemProperties(path: string): Promise<void> {
  if (!hasTauri) {
    console.log("[demo] open_system_properties", path);
    return;
  }
  return invoke<void>("open_system_properties", { path });
}

/** Opens a file with a specific program (empty = system default). */
export function openWith(path: string, program: string): void {
  if (!hasTauri) {
    console.log("[demo] open_with", path, program);
    return;
  }
  void invoke<void>("open_with", { path, program });
}

/** Requests the Git status of a folder; runs asynchronously in the backend
 *  in the background (blocks neither folder opening nor other IPC calls) and
 *  delivers the result via `onGitStatusReady` (event `git-support-ready`). */
export function requestGitStatus(path: string): void {
  if (!hasTauri) {
    // Demo: deliver the result asynchronously as well, like the original.
    queueMicrotask(() =>
      gitReadySubs.forEach((f) => f(path, demo.demoGitStatus(path))),
    );
    return;
  }
  void invoke<void>("git_status_watch", { path });
}

/** Opens a terminal in the folder (empty `program` = system default). */
export function openTerminal(path: string, program?: string): void {
  const prog = program?.trim() ? program.trim() : null;
  if (!hasTauri) {
    console.log("[demo] open_terminal", path, prog);
    return;
  }
  void invoke<void>("open_terminal", { path, program: prog });
}

/** Lists the contents of an archive at level `inner` (with a password if needed). */
export async function listArchive(
  archive: string,
  inner: string,
  password?: string,
): Promise<DirListing> {
  if (!hasTauri) return demo.demoArchive(archive, inner);
  return invoke<DirListing>("list_archive", {
    archive,
    inner,
    password: password ?? null,
  });
}
