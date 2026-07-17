// ============================================================
// Execution of the file operations.
// Copy/move/paste run concurrently as a transfer (progress
// via events); delete/mkdir are short synchronous operations.
// ============================================================

import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useOps } from "@/store/opsStore";
import { useTransfers } from "@/store/transfersStore";
import { useClipboard } from "@/store/clipboardStore";
import { PenLine, Trash2 } from "lucide-react";
import {
  deleteEntries,
  makeDir,
  openPath,
  openWith,
  quickLook,
  renameEntry,
  startExtract,
  startPack,
  startTransfer,
  trashEntries,
} from "@/ipc/client";
import { joinPath } from "@/lib/path";
import { splitName } from "@/lib/format";
import { isArchive } from "@/lib/archive";
import {
  archivePassword,
  isPasswordError,
  promptArchivePassword,
  PW_WRONG,
} from "@/features/commander/archivePw";
import type { OpDone } from "@/ipc/bindings";
import { openFileBrowser } from "@/store/fileBrowserStore";
import {
  programForExtension,
  translate,
  useSettings,
} from "@/store/settingsStore";

const other = (side: Side): Side => (side === "left" ? "right" : "left");

/** Base names of the target set (selection or cursor). */
function selectedNames(side: Side): string[] {
  const p = panelOf(usePanes.getState(), side);
  if (p.selected.size > 0) return [...p.selected];
  const cur = p.entries[p.cursor];
  return cur && !cur.parent ? [cur.name] : [];
}

function newId(): string {
  return typeof crypto !== "undefined" && crypto.randomUUID
    ? crypto.randomUUID()
    : `t${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/** Full paths of the target set (selection or cursor). */
export function targetPaths(side: Side): string[] {
  const p = panelOf(usePanes.getState(), side);
  const names =
    p.selected.size > 0
      ? [...p.selected]
      : (() => {
          const cur = p.entries[p.cursor];
          return cur && !cur.parent ? [cur.name] : [];
        })();
  return names.map((n) => joinPath(p.path, n));
}

/** Reload the active tabs of both sides (after an operation completes).
 *  Preserves scroll position/selection (TICKET-006) and selects newly
 *  added entries (target of a copy/move/paste operation). */
export async function reloadBoth(): Promise<void> {
  const s = usePanes.getState();
  await Promise.all([
    s.refresh("left", { reveal: true }),
    s.refresh("right", { reveal: true }),
  ]);
}

function reportErrors(errors: string[]): void {
  if (errors.length === 0) return;
  useOps.getState().requestConfirm({
    title: translate("op.errors"),
    message: errors.slice(0, 8).join("\n"),
    onConfirm: () => {},
  });
}

// ---------- Transfers (concurrent) ----------

// ----- Copy queue: process operations one after another -----

interface QueuedParams {
  op: "copy" | "move";
  sources: string[];
  dest: string;
  limit: number;
  verify: boolean;
  bufKb: number;
  threads: number;
}
/** Parameters of waiting (not yet started) transfers per id. */
const queuedParams = new Map<string, QueuedParams>();

/** Whether a copy/move operation is currently active (not waiting/done). */
function hasActiveTransfer(exceptId?: string): boolean {
  return useTransfers
    .getState()
    .transfers.some(
      (t) =>
        t.id !== exceptId &&
        (t.op === "copy" || t.op === "move") &&
        !t.done &&
        !t.queued,
    );
}

function beginTransfer(op: "copy" | "move", sources: string[], dest: string): void {
  if (!sources.length) return;
  const st = useSettings.getState();
  const limit = Math.max(0, Math.round(st.speedLimit)) * 1024; // KB/s → Bytes/s
  const verify = st.verifyCopies;
  const bufKb = st.bufferSizeKb;
  const threads = st.copyThreads;
  const id = newId();
  useTransfers.getState().start(op, id);

  // Queue active and an operation already underway → enqueue.
  if (st.queueTransfers && hasActiveTransfer(id)) {
    queuedParams.set(id, { op, sources, dest, limit, verify, bufKb, threads });
    useTransfers.getState().setQueued(id, true);
  } else {
    startTransfer(op, id, sources, dest, limit, verify, bufKb, threads);
  }
}

/** Starts the next waiting transfer, provided none is running. */
export function startNextQueued(): void {
  if (hasActiveTransfer()) return;
  const next = useTransfers.getState().transfers.find((t) => t.queued);
  if (!next) return;
  const params = queuedParams.get(next.id);
  if (!params) return;
  queuedParams.delete(next.id);
  useTransfers.getState().setQueued(next.id, false);
  startTransfer(
    params.op,
    next.id,
    params.sources,
    params.dest,
    params.limit,
    params.verify,
    params.bufKb,
    params.threads,
  );
}

/** Removes a still-waiting transfer from the queue (cancel before start). */
export function dequeueTransfer(id: string): void {
  queuedParams.delete(id);
  useTransfers.getState().remove(id);
}

/** Pending extract operations (for a password retry after fs-done). */
interface ExtractReq {
  archive: string;
  base: string;
  names: string[];
  dest: string;
}
const pendingExtracts = new Map<string, ExtractReq>();

/** Extracts from an archive (level `base`) to `dest`. */
function beginExtract(
  archive: string,
  base: string,
  names: string[],
  dest: string,
): void {
  const id = newId();
  pendingExtracts.set(id, { archive, base, names, dest });
  useTransfers.getState().start("extract", id);
  startExtract(id, archive, base, names, dest, archivePassword(archive));
}

/**
 * Handles a completed extract operation: if the password was missing or
 * wrong, it is requested and the operation retried. Returns true
 * when a password retry was initiated (the caller then
 * skips the normal error display).
 */
export function handleExtractDone(d: OpDone): boolean {
  if (d.op !== "extract") return false;
  const req = pendingExtracts.get(d.id);
  pendingExtracts.delete(d.id);
  if (!req || d.cancelled) return false;
  const needsPassword = d.errors.some((e) => isPasswordError(e));
  if (!needsPassword) return false;

  const wrong = d.errors.some((e) => e.includes(PW_WRONG));
  useTransfers.getState().remove(d.id); // gescheiterten Vorgang entfernen
  promptArchivePassword(req.archive, wrong, () => {
    beginExtract(req.archive, req.base, req.names, req.dest);
  });
  return true;
}

/** Target folder of the opposite side; blocks writing into an archive. */
function destOf(side: Side): string | null {
  const dst = panelOf(usePanes.getState(), other(side));
  return dst.archive ? null : dst.path;
}

export function runCopy(side: Side): void {
  const src = panelOf(usePanes.getState(), side);
  const dest = destOf(side);
  if (dest === null) return; // Kopieren in ein Archiv wird nicht unterstützt
  if (src.archive) {
    beginExtract(src.archive, src.path, selectedNames(side), dest);
  } else {
    beginTransfer("copy", targetPaths(side), dest);
  }
}

export function runMove(side: Side): void {
  const src = panelOf(usePanes.getState(), side);
  const dest = destOf(side);
  if (dest === null) return;
  // "Moving" out of an archive = extracting (the archive stays unchanged).
  if (src.archive) {
    beginExtract(src.archive, src.path, selectedNames(side), dest);
  } else {
    beginTransfer("move", targetPaths(side), dest);
  }
}

/**
 * Pastes the clipboard. Without `destDir` into the current folder;
 * with `destDir` (right-click on a folder) directly into that folder.
 */
export function runPaste(side: Side, destDir?: string): void {
  const clip = useClipboard.getState();
  if (!clip.mode || clip.names.length === 0) return;
  const dst = panelOf(usePanes.getState(), side);
  if (dst.archive) return; // nicht in ein Archiv einfügen
  const target = destDir ?? dst.path;
  const sources = clip.names.map((n) => joinPath(clip.sourcePath, n));
  const op = clip.mode === "cut" ? "move" : "copy";
  beginTransfer(op, sources, target);
  if (clip.mode === "cut") clip.clear();
}

/** "Extract": selected archives (real FS) completely into the opposite side. */
export function runExtractArchive(side: Side): void {
  const src = panelOf(usePanes.getState(), side);
  if (src.archive) return;
  const dest = destOf(side);
  if (dest === null) return;
  const archives = selectedNames(side).filter(isArchive);
  for (const name of archives) {
    beginExtract(joinPath(src.path, name), "", [], dest);
  }
}

/** "Pack": the selection into a new ZIP on the opposite side. */
export function runPack(side: Side): void {
  const src = panelOf(usePanes.getState(), side);
  if (src.archive) return;
  const dest = destOf(side);
  if (dest === null) return;
  const sources = targetPaths(side);
  if (!sources.length) return;

  useOps.getState().requestPrompt({
    title: translate("archive.title"),
    label: translate("archive.label"),
    initial: "archiv.zip",
    confirmLabel: translate("op.create"),
    onSubmit: (name) => {
      let zipName = name.trim();
      if (!zipName) return;
      if (!zipName.toLowerCase().endsWith(".zip")) zipName += ".zip";
      const id = newId();
      useTransfers.getState().start("pack", id);
      startPack(id, sources, joinPath(dest, zipName));
    },
  });
}

/** "Edit" (F4): open the file with the default program. */
export function runEdit(side: Side): void {
  const p = panelOf(usePanes.getState(), side);
  if (p.archive) return;
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent || cur.is_dir) return;
  openPath(joinPath(p.path, cur.name));
}

/**
 * Native macOS Quick Look preview of the cursor entry (Space).
 * Returns false when not possible (archive/"..") → the caller can select instead.
 */
export function runQuickLook(side: Side): boolean {
  const p = panelOf(usePanes.getState(), side);
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent || p.archive) return false;
  quickLook(joinPath(p.path, cur.name));
  return true;
}

/** Full path of the cursor file, or null (folder/".."/archive). */
function cursorFile(side: Side): string | null {
  const p = panelOf(usePanes.getState(), side);
  if (p.archive) return null;
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent || cur.is_dir) return null;
  return joinPath(p.path, cur.name);
}

/**
 * "Open in editor": program by mapping (extension) → global
 * default editor → system default. Alternative to F4.
 */
export function runEditWith(side: Side): void {
  const p = panelOf(usePanes.getState(), side);
  if (p.archive) return;
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent || cur.is_dir) return;
  const prog = programForExtension(splitName(cur).ext);
  const program = prog?.path ?? useSettings.getState().defaultEditor;
  openWith(joinPath(p.path, cur.name), program); // leer = System-Standard
}

/** Open a file with a specific program (context menu). */
export function runOpenWithProgram(side: Side, program: string): void {
  const full = cursorFile(side);
  if (full) openWith(full, program);
}

/** "Other program …": choose a program via the file browser and open. */
export function runOpenWithBrowse(side: Side): void {
  const full = cursorFile(side);
  if (!full) return;
  openFileBrowser({
    title: translate("ctx.openWithOther"),
    selectMode: "file",
    onPick: (program) => openWith(full, program),
  });
}

// ---------- Short synchronous operations ----------

async function withBusy<T>(fn: () => Promise<T>): Promise<T> {
  const ops = useOps.getState();
  ops.setBusy(true);
  try {
    return await fn();
  } finally {
    ops.setBusy(false);
  }
}

/**
 * Delete (F8/Del): by default to the trash (restorable).
 * With `permanent` (Shift) or the trash disabled, it deletes permanently.
 */
export function runDelete(side: Side, opts?: { permanent?: boolean }): void {
  const paths = targetPaths(side);
  if (!paths.length) return;

  const n = String(paths.length);
  const toTrash = useSettings.getState().useTrash && !opts?.permanent;

  if (toTrash) {
    useOps.getState().requestConfirm({
      title: translate("op.trash.title"),
      message:
        translate("op.trash.confirm").replace("{n}", n) +
        "\n\n" +
        translate("op.trash.hint"),
      icon: Trash2,
      confirmLabel: translate("op.trash.button"),
      onConfirm: () =>
        void withBusy(async () => {
          const res = await trashEntries(paths);
          await reloadBoth();
          reportErrors(res.errors);
        }),
    });
    return;
  }

  useOps.getState().requestConfirm({
    title: translate("op.delete.title"),
    message: translate("op.delete.confirm").replace("{n}", n),
    danger: true,
    confirmLabel: translate("op.delete.button"),
    onConfirm: () =>
      void withBusy(async () => {
        const res = await deleteEntries(paths);
        await reloadBoth();
        reportErrors(res.errors);
      }),
  });
}

/** Rename (F2): renames the cursor entry in the current folder. */
export function runRename(side: Side): void {
  const p = panelOf(usePanes.getState(), side);
  if (p.archive) return; // in Archiven nicht umbenennen
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent) return;

  const oldName = cur.name;
  const base = p.path;
  // For files, preselect only the base name (before the extension).
  const dot = !cur.is_dir ? oldName.lastIndexOf(".") : -1;
  const selEnd = dot > 0 ? dot : oldName.length;

  useOps.getState().requestPrompt({
    title: translate("op.rename.title"),
    label: translate("op.rename.label"),
    initial: oldName,
    confirmLabel: translate("op.rename.confirm"),
    icon: PenLine,
    selectRange: [0, selEnd],
    onSubmit: (value) => {
      const name = value.trim();
      if (!name || name === oldName) return;
      if (name.includes("/") || name.includes("\\")) {
        reportErrors([translate("op.rename.invalid")]);
        return;
      }
      // Report a name conflict localized up front (the backend also checks).
      const exists = panelOf(usePanes.getState(), side).entries.some(
        (e) => !e.parent && e.name === name,
      );
      if (exists) {
        reportErrors([translate("op.collision.exists").replace("{name}", name)]);
        return;
      }
      void withBusy(async () => {
        try {
          await renameEntry(joinPath(base, oldName), joinPath(base, name));
          await usePanes.getState().refresh(side, { reveal: true });
        } catch (e) {
          reportErrors([String(e)]);
        }
      });
    },
  });
}

export function runMkdir(side: Side): void {
  useOps.getState().requestPrompt({
    title: translate("op.mkdir.title"),
    label: translate("op.mkdir.label"),
    initial: "",
    onSubmit: (name) => {
      const trimmed = name.trim();
      if (!trimmed) return;
      const base = panelOf(usePanes.getState(), side).path;
      void withBusy(async () => {
        await makeDir(joinPath(base, trimmed));
        await usePanes.getState().refresh(side, { reveal: true });
      });
    },
  });
}
