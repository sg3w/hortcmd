// ============================================================
// Ausführung der Dateioperationen.
// Copy/Move/Paste laufen nebenläufig als Transfer (Fortschritt
// via Events); Delete/Mkdir sind kurze synchrone Operationen.
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

/** Basisnamen der Zielmenge (Auswahl oder Cursor). */
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

/** Vollständige Pfade der Zielmenge (Auswahl oder Cursor). */
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

/** Aktive Tabs beider Seiten neu laden (nach Abschluss eines Vorgangs).
 *  Bewahrt Scrollposition/Auswahl (TICKET-006) und markiert neu
 *  hinzugekommene Einträge (Ziel eines Kopier-/Verschiebe-/Einfüge-Vorgangs). */
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

// ---------- Transfers (nebenläufig) ----------

// ----- Kopier-Queue: Vorgänge nacheinander abarbeiten -----

interface QueuedParams {
  op: "copy" | "move";
  sources: string[];
  dest: string;
  limit: number;
  verify: boolean;
  bufKb: number;
  threads: number;
}
/** Parameter wartender (noch nicht gestarteter) Transfers je id. */
const queuedParams = new Map<string, QueuedParams>();

/** Ob gerade ein Kopier-/Verschiebevorgang aktiv läuft (nicht wartend/fertig). */
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

  // Warteschlange aktiv und schon ein Vorgang unterwegs → einreihen.
  if (st.queueTransfers && hasActiveTransfer(id)) {
    queuedParams.set(id, { op, sources, dest, limit, verify, bufKb, threads });
    useTransfers.getState().setQueued(id, true);
  } else {
    startTransfer(op, id, sources, dest, limit, verify, bufKb, threads);
  }
}

/** Startet den nächsten wartenden Transfer, sofern gerade keiner läuft. */
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

/** Entfernt einen noch wartenden Transfer aus der Queue (Abbruch vor Start). */
export function dequeueTransfer(id: string): void {
  queuedParams.delete(id);
  useTransfers.getState().remove(id);
}

/** Offene Entpack-Vorgänge (für Passwort-Wiederholung nach fs-done). */
interface ExtractReq {
  archive: string;
  base: string;
  names: string[];
  dest: string;
}
const pendingExtracts = new Map<string, ExtractReq>();

/** Entpackt aus einem Archiv (Ebene `base`) nach `dest`. */
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
 * Behandelt einen abgeschlossenen Entpack-Vorgang: Fehlte/stimmte das
 * Passwort nicht, wird es erfragt und der Vorgang wiederholt. Gibt true
 * zurück, wenn eine Passwort-Wiederholung eingeleitet wurde (der Aufrufer
 * überspringt dann die normale Fehleranzeige).
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

/** Zielordner der Gegenseite; blockiert das Schreiben in ein Archiv. */
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
  // Aus einem Archiv „verschieben" = entpacken (Archiv bleibt unverändert).
  if (src.archive) {
    beginExtract(src.archive, src.path, selectedNames(side), dest);
  } else {
    beginTransfer("move", targetPaths(side), dest);
  }
}

/**
 * Fügt die Zwischenablage ein. Ohne `destDir` in den aktuellen Ordner;
 * mit `destDir` (Rechtsklick auf einen Ordner) direkt in diesen Ordner.
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

/** „Entpacken": ausgewählte Archive (reales FS) komplett in die Gegenseite. */
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

/** „Packen": Auswahl in ein neues ZIP in der Gegenseite. */
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

/** „Bearbeiten" (F4): Datei mit dem Standardprogramm öffnen. */
export function runEdit(side: Side): void {
  const p = panelOf(usePanes.getState(), side);
  if (p.archive) return;
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent || cur.is_dir) return;
  openPath(joinPath(p.path, cur.name));
}

/**
 * Native macOS-Quick-Look-Vorschau des Cursor-Eintrags (Leertaste).
 * Liefert false, wenn nicht möglich (Archiv/„..“) → Aufrufer kann markieren.
 */
export function runQuickLook(side: Side): boolean {
  const p = panelOf(usePanes.getState(), side);
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent || p.archive) return false;
  quickLook(joinPath(p.path, cur.name));
  return true;
}

/** Vollständiger Pfad der Cursor-Datei, oder null (Ordner/„..“/Archiv). */
function cursorFile(side: Side): string | null {
  const p = panelOf(usePanes.getState(), side);
  if (p.archive) return null;
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent || cur.is_dir) return null;
  return joinPath(p.path, cur.name);
}

/**
 * „Im Editor öffnen": Programm nach Zuordnung (Endung) → globaler
 * Standard-Editor → System-Standard. Alternative zu F4.
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

/** Datei mit einem bestimmten Programm öffnen (Kontextmenü). */
export function runOpenWithProgram(side: Side, program: string): void {
  const full = cursorFile(side);
  if (full) openWith(full, program);
}

/** „Anderes Programm …": Programm über den Dateibrowser wählen und öffnen. */
export function runOpenWithBrowse(side: Side): void {
  const full = cursorFile(side);
  if (!full) return;
  openFileBrowser({
    title: translate("ctx.openWithOther"),
    selectMode: "file",
    onPick: (program) => openWith(full, program),
  });
}

// ---------- Kurze synchrone Operationen ----------

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
 * Löschen (F8/Entf): standardmäßig in den Papierkorb (wiederherstellbar).
 * Mit `permanent` (Shift) oder deaktiviertem Papierkorb wird endgültig gelöscht.
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

/** Umbenennen (F2): benennt den Cursor-Eintrag im aktuellen Ordner um. */
export function runRename(side: Side): void {
  const p = panelOf(usePanes.getState(), side);
  if (p.archive) return; // in Archiven nicht umbenennen
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent) return;

  const oldName = cur.name;
  const base = p.path;
  // Bei Dateien nur den Basisnamen (vor der Endung) vorwählen.
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
      // Namenskonflikt vorab lokalisiert melden (Backend prüft zusätzlich).
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
