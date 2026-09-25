import type { OperationPlan } from "@/ipc/bindings/OperationPlan";
import type { RecoverySettings } from "@/ipc/bindings/RecoverySettings";
// ============================================================
// Execution of the file operations.
// Copy/move/paste run concurrently as a transfer (progress
// via events); delete/mkdir are short synchronous operations.
// ============================================================

import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useOps } from "@/store/opsStore";
import { useTransfers } from "@/store/transfersStore";
import { useClipboard } from "@/store/clipboardStore";
import { PenLine, ShieldAlert, Trash2 } from "lucide-react";
import {
  hasTauri,
  queueRecovery,
  discardRecovery,
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
import { allows } from "@/store/capsStore";
import { guard, guardPair } from "./guard";
import { splitName } from "@/lib/format";
import { isArchive } from "@/lib/archive";
import {
  archivePassword,
  promptArchivePassword,
  PW_REQUIRED,
  PW_WRONG,
} from "@/features/commander/archivePw";
import type { ItemError, Leftovers, OpDone } from "@/ipc/bindings";
import { ipcError, isError } from "@/lib/appError";
import { describeErrorFull, describeItemErrors } from "@/store/errorText";
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

/** Shows one already-phrased message the frontend produced itself. */
function reportMessage(message: string): void {
  useOps.getState().requestConfirm({
    title: translate("op.errors"),
    message,
    onConfirm: () => {},
  });
}

/**
 * Shows what failed in a batch. The messages are produced *here*, from
 * the codes the backend sent (TICKET-036), so they arrive in the user's
 * language; a cancelled item is dropped rather than reported as a
 * failure.
 */
function reportErrors(errors: ItemError[]): void {
  const lines = describeItemErrors(errors);
  if (lines.length === 0) return;
  useOps.getState().requestConfirm({
    title: translate("op.errors"),
    message: lines.slice(0, 8).join("\n"),
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
  /** Safety policy captured at enqueue time (TICKET-029). */
  safe: boolean;
  checkSpace?: boolean;
}
/** Parameters of waiting (not yet started) transfers per id. */
const queuedParams = new Map<string, QueuedParams>();
const persistedIntents = new Map<string, Promise<boolean>>();

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

function transferSettings(): RecoverySettings {
  const st = useSettings.getState();
  return { limit: Math.max(0, Math.round(st.speedLimit)) * 1024, verify: st.verifyCopies, bufKb: st.bufferSizeKb, threads: st.copyThreads, safe: st.safeTransfers, checkSpace: st.checkTransferSpace };
}
function beginTransfer(op: "copy" | "move", sources: string[], dest: string, onApproved?: () => void): void {
  if (!sources.length) return;
  useOps.getState().requestConfirm({
    title: translate(op === "copy" ? "fn.copy" : "fn.move"),
    message: `${sources.join("\n")} → ${dest}`,
    undo: { kind: op, sources, target: dest, directory: true },
    preflight: { kind: op, sources, destination: dest, settings: transferSettings() },
    onConfirm: plan => { beginApprovedTransfer(op, sources, dest, plan); onApproved?.(); },
  });
}

function beginApprovedTransfer(op: "copy" | "move", sources: string[], dest: string, plan?: OperationPlan): void {
  if (!sources.length) return;
  const st = useSettings.getState();
  const settings = plan?.settings ?? transferSettings();
  const { limit, verify, bufKb, threads, safe, checkSpace } = settings;
  const id = newId();
  useTransfers.getState().start(op, id);

  if (hasTauri()) {
    persistedIntents.set(id, queueRecovery(id, op, sources, dest, { limit, verify, bufKb, threads, safe, checkSpace }, plan?.digest).then(() => true, error => {
      useTransfers.getState().finish({ id, op, ok: 0, errors: [{ item: "", error: ipcError(error) }], cancelled: false, mode: safe ? "safe" : "direct" });
      startNextQueued();
      return false;
    }));
  }
  // Queue active and an operation already underway → enqueue.
  if (st.queueTransfers && hasActiveTransfer(id)) {
    queuedParams.set(id, { op, sources, dest, limit, verify, bufKb, threads, safe, checkSpace });
    useTransfers.getState().setQueued(id, true);
  } else {
    const start = () => startTransfer(op, id, sources, dest, limit, verify, bufKb, threads, safe, checkSpace);
    const persisted = persistedIntents.get(id);
    if (persisted) void persisted.then(ok => { persistedIntents.delete(id); if (ok) start(); });
    else start();
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
  const start = () => startTransfer(
    params.op,
    next.id,
    params.sources,
    params.dest,
    params.limit,
    params.verify,
    params.bufKb,
    params.threads,
    params.safe,
    params.checkSpace,
  );
  const persisted = persistedIntents.get(next.id);
  if (persisted) void persisted.then(ok => {
    persistedIntents.delete(next.id);
    if (ok && useTransfers.getState().transfers.some(t => t.id === next.id && !t.done)) start();
  });
  else start();
}

/** Removes a still-waiting transfer from the queue (cancel before start). */
export function dequeueTransfer(id: string): void {
  if(id.startsWith("plugin-")) {
    void import("@/ipc/domains/plugins").then(module=>module.pluginTransferCancel(id)).catch(error=>reportErrors([{item:"",error:ipcError(error)}]));
    useTransfers.getState().remove(id);return;
  }
  queuedParams.delete(id);
  if (hasTauri()) {
    const persisted = persistedIntents.get(id) ?? Promise.resolve(true);
    void persisted.then(ok => ok ? discardRecovery(id) : undefined).catch(error => reportErrors([{ item: "", error: ipcError(error) }]));
  }
  persistedIntents.delete(id);
  useTransfers.getState().remove(id);
}

/**
 * Remains of an interrupted failure-safe transfer (TICKET-029) found in
 * a destination folder. A staging or backup entry can hold the only
 * copy of a file, so nothing is removed on a guess: the user is told
 * what was found and where, and deletes it only on request.
 */
export function confirmLeftovers(l: Leftovers): void {
  if (!l.paths.length) return;
  useOps.getState().requestConfirm({
    title: translate("op.leftovers.title"),
    message:
      translate("op.leftovers.message")
        .replace("{n}", String(l.paths.length))
        .replace("{dir}", l.dir) +
      "\n\n" +
      l.paths.join("\n"),
    icon: ShieldAlert,
    confirmLabel: translate("op.leftovers.button"),
    danger: true,
    onConfirm: () =>
      void withBusy(async () => {
        const res = await deleteEntries(l.paths);
        await reloadBoth();
        reportErrors(res.errors);
      }),
  });
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
  id = newId(),
): void {
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
  // Branching on codes, not on message text (TICKET-036): the two
  // password outcomes are distinct answers, and "wrong" is what makes
  // the prompt say so.
  const wrong = d.errors.some((e) => isError(e.error, PW_WRONG));
  if (!wrong && !d.errors.some((e) => isError(e.error, PW_REQUIRED))) {
    return false;
  }
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
    // Copying out of an archive is an extraction (TICKET-031).
    if (!guardPair(side, src.archive, "archiveExtractFrom", dest, "archiveExtractTo")) {
      return;
    }
    beginExtract(src.archive, src.path, selectedNames(side), dest);
  } else {
    if (!guardPair(side, src.path, "copyFrom", dest, "copyTo")) return;
    beginTransfer("copy", targetPaths(side), dest);
  }
}

export function runMove(side: Side): void {
  const src = panelOf(usePanes.getState(), side);
  const dest = destOf(side);
  if (dest === null) return;
  // "Moving" out of an archive = extracting (the archive stays unchanged).
  if (src.archive) {
    if (!guardPair(side, src.archive, "archiveExtractFrom", dest, "archiveExtractTo")) {
      return;
    }
    beginExtract(src.archive, src.path, selectedNames(side), dest);
  } else {
    if (!guardPair(side, src.path, "move", dest, "copyTo")) return;
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
  const op = clip.mode === "cut" ? "move" : "copy";
  // The clipboard's source folder is a location of its own and may have
  // gone offline since the copy (TICKET-031).
  if (
    !guardPair(
      side,
      clip.sourcePath,
      op === "move" ? "move" : "copyFrom",
      target,
      "copyTo",
    )
  ) {
    return;
  }
  const sources = clip.names.map((n) => joinPath(clip.sourcePath, n));
  beginTransfer(op, sources, target, () => {
    const current = useClipboard.getState();
    if (clip.mode === "cut" && current.mode === clip.mode && current.sourcePath === clip.sourcePath && current.names === clip.names) current.clear();
  });
}

/** "Extract": selected archives (real FS) completely into the opposite side. */
export function runExtractArchive(side: Side): void {
  const src = panelOf(usePanes.getState(), side);
  if (src.archive) return;
  const dest = destOf(side);
  if (dest === null) return;
  const archives = selectedNames(side).filter(isArchive);
  if (
    archives.length &&
    !guardPair(side, src.path, "archiveExtractFrom", dest, "archiveExtractTo")
  ) {
    return;
  }
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
  if (!guardPair(side, src.path, "archivePackFrom", dest, "archivePackTo")) {
    return;
  }

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
  if (!guard(side, p.path, "systemOpen")) return;
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
  // Space falls back to "select the row" where Quick Look is not
  // available, so a missing capability is not an error here — it is the
  // other meaning of the key.
  if (!allows(p.path, "quickLook")) return false;
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
  if (!guard(side, p.path, "openWith")) return;
  const prog = programForExtension(splitName(cur).ext);
  const program = prog?.path ?? useSettings.getState().defaultEditor;
  openWith(joinPath(p.path, cur.name), program); // leer = System-Standard
}

/** Open a file with a specific program (context menu). */
export function runOpenWithProgram(side: Side, program: string): void {
  const full = cursorFile(side);
  if (!full) return;
  if (!guard(side, panelOf(usePanes.getState(), side).path, "openWith")) return;
  openWith(full, program);
}

/** "Other program …": choose a program via the file browser and open. */
export function runOpenWithBrowse(side: Side): void {
  const full = cursorFile(side);
  if (!full) return;
  if (!guard(side, panelOf(usePanes.getState(), side).path, "openWith")) return;
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

  const dir = panelOf(usePanes.getState(), side).path;
  if (!guard(side, dir, "deletePermanent")) return;

  const n = String(paths.length);
  // A location without an OS trash (every remote one so far) deletes
  // permanently instead — with the permanent-delete confirmation, so
  // the irreversible step is never taken behind a "moves to trash"
  // wording (TICKET-031).
  const toTrash =
    useSettings.getState().useTrash &&
    !opts?.permanent &&
    allows(dir, "trash");

  if (toTrash) {
    useOps.getState().requestConfirm({
      title: translate("op.trash.title"),
      message:
        translate("op.trash.confirm").replace("{n}", n) +
        "\n\n" +
        translate("op.trash.hint") + "\n\n" + translate("undo.trash"),
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
    message: translate("op.delete.confirm").replace("{n}", n) + "\n\n" + translate("undo.irreversible"),
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

  if (!guard(side, p.path, "move")) return;

  const oldName = cur.name;
  const base = p.path;
  // For files, preselect only the base name (before the extension).
  const dot = !cur.is_dir ? oldName.lastIndexOf(".") : -1;
  const selEnd = dot > 0 ? dot : oldName.length;

  useOps.getState().requestPrompt({
    title: translate("op.rename.title"),
    label: translate("op.rename.label"),
    undo: { kind: "rename", source: joinPath(base, oldName), base },
    initial: oldName,
    confirmLabel: translate("op.rename.confirm"),
    icon: PenLine,
    selectRange: [0, selEnd],
    onSubmit: (value) => {
      const name = value.trim();
      if (!name || name === oldName) return;
      if (name.includes("/") || name.includes("\\")) {
        reportMessage(translate("op.rename.invalid"));
        return;
      }
      // Report a name conflict localized up front (the backend also checks).
      const exists = panelOf(usePanes.getState(), side).entries.some(
        (e) => !e.parent && e.name === name,
      );
      if (exists) {
        reportMessage(translate("op.collision.exists").replace("{name}", name));
        return;
      }
      void withBusy(async () => {
        try {
          await renameEntry(joinPath(base, oldName), joinPath(base, name));
          await usePanes
            .getState()
            .refresh(side, { reveal: true, names: [name] });
        } catch (e) {
          reportMessage(describeErrorFull(e));
        }
      });
    },
  });
}

export function runMkdir(side: Side): void {
  if (!guard(side, panelOf(usePanes.getState(), side).path, "create")) return;
  useOps.getState().requestPrompt({
    title: translate("op.mkdir.title"),
    label: translate("op.mkdir.label"),
    undo: { kind: "mkdir", source: "", base: panelOf(usePanes.getState(), side).path },
    initial: "",
    onSubmit: (name) => {
      const trimmed = name.trim();
      if (!trimmed) return;
      const base = panelOf(usePanes.getState(), side).path;
      void withBusy(async () => {
        await makeDir(joinPath(base, trimmed));
        // Reveal by name, not by diff: the folder stays under the cursor even
        // if the watcher refreshed the listing in between (TICKET-014).
        await usePanes
          .getState()
          .refresh(side, { reveal: true, names: [trimmed] });
      });
    },
  });
}


/** Executes an already journaled, freshly revalidated recovery request. */
export function startRecovered(request: import("@/ipc/bindings/RecoveryLaunch").RecoveryLaunch): void {
  const { id, kind, sources, destination, settings } = request;
  if (kind === "extract") {
    beginExtract(sources[0], request.base, request.names, destination, id);
    return;
  }
  useTransfers.getState().start(kind, id);
  if (kind === "pack") startPack(id, sources, destination);
  else startTransfer(kind, id, sources, destination, settings.limit, settings.verify, settings.bufKb, settings.threads, settings.safe, settings.checkSpace);
}
