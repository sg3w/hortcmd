// ============================================================
// Dialogs & progress display for file operations.
// Confirmation (delete/error), input (new folder),
// progress overlay (fed from fs-progress events).
// ============================================================

import { TransferPreflight, useTransferPreflight } from "./TransferPreflight";
import { joinPath } from "@/lib/path";
import { useUndoClassification } from "./useUndoClassification";
import { useEffect, useId, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { AlertTriangle, FileWarning, FolderPlus, XCircle } from "lucide-react";
import { useUi } from "@/store/uiStore";
import { useOps } from "@/store/opsStore";
import { useTransfers } from "@/store/transfersStore";
import { cancelTransfer, resolveCollision } from "@/ipc/client";
import { useT } from "@/store/settingsStore";
import { useDialogFocusReturn } from "./useDialogFocusReturn";
import { cn } from "@/lib/cn";
import { consumeDialogKey, markDialogKey } from "@/lib/keys";

export function OperationDialogs() {
  return (
    <>
      <ConfirmDialog />
      <PromptDialog />
      <CollisionDialog />
      <ProgressOverlay />
      <ForceCloseDialog />
    </>
  );
}

// ---------- Name collision ----------

function CollisionDialog() {
  const head = useOps((s) => s.collisions[0] ?? null);
  const shiftCollision = useOps((s) => s.shiftCollision);
  const t = useT();
  const [applyAll, setApplyAll] = useState(false);

  // Reset "for all" when switching to a new collision.
  useEffect(() => {
    setApplyAll(false);
  }, [head?.req_id]);

  const answer = (action: "overwrite" | "rename" | "skip") => {
    if (!head) return;
    resolveCollision(head.req_id, action, applyAll);
    shiftCollision();
  };

  const name = head ? basename(head.path) : "";

  return (
    <SimpleDialog
      open={!!head}
      onOpenChange={() => {}}
      overlayZ="z-[62]"
      contentZ="z-[63]"
      className="w-[440px]"
    >
          <Dialog.Title className="flex items-center gap-2 border-b border-edge px-4 py-2.5 text-text">
            <FileWarning size={16} className="text-amber-400" />
            {t("op.collision.title")}
          </Dialog.Title>
          <Dialog.Description className="px-4 py-3 text-[13px] text-text">
            {t("op.collision.exists").replace("{name}", name)}
          </Dialog.Description>
          <label className="flex cursor-pointer items-center gap-2 px-4 pb-2 text-[12px] text-dim">
            <input
              type="checkbox"
              checked={applyAll}
              onChange={(e) => setApplyAll(e.target.checked)}
              className="accent-[color:var(--accent)]"
            />
            {t("op.collision.applyAll")}
          </label>
          <div className="flex justify-end gap-2 border-t border-edge px-4 py-2.5">
            <button
              onClick={() => answer("skip")}
              className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
            >
              {t("op.collision.skip")}
            </button>
            <button
              onClick={() => answer("rename")}
              className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
            >
              {t("op.collision.rename")}
            </button>
            <button
              onClick={() => answer("overwrite")}
              className="rounded bg-amber-500 px-3 py-1 text-[13px] text-white hover:brightness-110"
            >
              {t("op.collision.overwrite")}
            </button>
          </div>
    </SimpleDialog>
  );
}

function basename(path: string): string {
  if (!path) return "";
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/**
 * The chrome the blocking dialogs in this file share: a modal
 * `Dialog.Root`, its overlay, and a centered `Dialog.Content` that returns
 * focus to wherever it came from. The fixed-size sibling of `AppDialog`
 * (no resizing, no stored size). Everything here used to be copied per
 * dialog, which is how `ProgressOverlay` ended up without a focus trap at
 * all — a chrome fix now lands in one place (ticket 348).
 */
function SimpleDialog({
  open,
  onOpenChange,
  overlayZ,
  contentZ,
  className,
  noDescription,
  onOpenAutoFocus,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  overlayZ: string;
  contentZ: string;
  /** Width and any per-dialog overflow behaviour. */
  className: string;
  /** Opts out of Radix's description wiring for a dialog without one. */
  noDescription?: boolean;
  /** Overrides where focus lands on open (Radix picks the first tabbable). */
  onOpenAutoFocus?: (event: Event) => void;
  children: React.ReactNode;
}) {
  const returnFocus = useDialogFocusReturn();
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className={cn("fixed inset-0 bg-black/50", overlayZ)} />
        <Dialog.Content
          onCloseAutoFocus={returnFocus}
          onOpenAutoFocus={onOpenAutoFocus}
          {...(noDescription ? { "aria-describedby": undefined } : {})}
          onKeyDownCapture={markDialogKey}
          className={cn(
            "fixed left-1/2 top-1/2 max-w-[92vw] -translate-x-1/2 -translate-y-1/2 rounded-lg border border-edge bg-panel shadow-2xl",
            contentZ,
            className,
          )}
        >
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

// ---------- Confirmation ----------

function ConfirmDialog() {
  const confirm = useOps((s) => s.confirm);
  const settingsOpen = useUi(s => s.settingsOpen);
  const undoClass = useUndoClassification(confirm?.undo);
  const preflight = useTransferPreflight(confirm?.preflight);
  const closeDialogs = useOps((s) => s.closeDialogs);
  const t = useT();
  const confirmButton = useRef<HTMLButtonElement>(null);

  const Icon = confirm?.icon ?? (confirm?.danger ? AlertTriangle : null);

  return (
    <SimpleDialog
      open={!!confirm && !settingsOpen}
      onOpenChange={(o) => !o && closeDialogs()}
      overlayZ="z-[60]"
      contentZ="z-[61]"
      className="max-h-[90vh] w-[420px] overflow-y-auto"
      // The confirm button owns the initial focus. Saying so beats relying
      // on React's `autoFocus` winning the race against Radix's "focus the
      // first tabbable", which another dialog closing in the same commit
      // can tip the other way (ticket 348).
      onOpenAutoFocus={(event) => {
        event.preventDefault();
        confirmButton.current?.focus();
      }}
    >
          <Dialog.Title className="flex items-center gap-2 border-b border-edge px-4 py-2.5 text-text">
            {Icon && (
              <Icon
                size={16}
                className={confirm?.danger ? "text-red-400" : "text-accent"}
              />
            )}
            {confirm?.title}
          </Dialog.Title>
          <Dialog.Description className="max-h-[240px] overflow-y-auto whitespace-pre-line px-4 py-3 text-[13px] text-text">
            {confirm?.message}
            {undoClass !== undefined && <span className="mt-3 block">{t(undoClass === null ? "undo.checking" : undoClass === "irreversible" ? "undo.irreversible" : "undo.conditional")}</span>}
          </Dialog.Description>
          {confirm?.preflight && <TransferPreflight {...preflight} />}
          <div className="flex justify-end gap-2 border-t border-edge px-4 py-2.5">
            <button
              onClick={closeDialogs}
              className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
            >
              {t("op.cancel")}
            </button>
            <button
              ref={confirmButton}
              disabled={undoClass === null || preflight.blocked}
              onClick={() => {
                useOps.getState().confirmDialog(preflight.plan ?? undefined);
              }}
              className={cn(
                "rounded px-3 py-1 text-[13px] text-white",
                confirm?.danger
                  ? "bg-red-500 hover:bg-red-600"
                  : "bg-accent hover:brightness-110",
              )}
            >
              {confirm?.confirmLabel ?? t("op.confirm")}
            </button>
          </div>
    </SimpleDialog>
  );
}

// ---------- Input (new folder) ----------

function PromptDialog() {
  const inputId = useId();
  const prompt = useOps((s) => s.prompt);
  const closeDialogs = useOps((s) => s.closeDialogs);
  const t = useT();
  const [value, setValue] = useState("");
  const [checked, setChecked] = useState(false);
  const undoTarget = prompt?.undo ? joinPath(prompt.undo.base, value.trim()) : "";
  const undoClass = useUndoClassification(prompt?.undo ? {
    kind: prompt.undo.kind, sources: [prompt.undo.kind === "mkdir" ? undoTarget : prompt.undo.source], target: undoTarget, directory: false,
  } : undefined);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (prompt) {
      setValue(prompt.initial);
      setChecked(prompt.checkbox?.initial ?? false);
      // focus after opening and preselect the given range if provided.
      requestAnimationFrame(() => {
        const el = inputRef.current;
        if (!el) return;
        el.focus();
        if (prompt.selectRange) {
          el.setSelectionRange(prompt.selectRange[0], prompt.selectRange[1]);
        }
      });
    }
  }, [prompt]);

  const valid = prompt?.validation === "any" ||
    (prompt?.validation === "nonEmpty" ? value.length > 0 : value.trim().length > 0);
  const canSubmit = !!prompt && undoClass !== null && valid;
  const submit = () => {
    if (!canSubmit) {
      inputRef.current?.focus();
      return;
    }
    // Close first, then execute: this way onSubmit can itself open a follow-up
    // dialog (e.g. an error message) without being overwritten immediately.
    const fn = prompt?.onSubmit;
    closeDialogs();
    fn?.(value, checked);
  };

  const Icon = prompt?.icon ?? FolderPlus;

  return (
    <SimpleDialog
      open={!!prompt}
      onOpenChange={(o) => !o && closeDialogs()}
      overlayZ="z-[60]"
      contentZ="z-[61]"
      className="w-[420px]"
      noDescription
    >
          <Dialog.Title className="flex items-center gap-2 border-b border-edge px-4 py-2.5 text-text">
            <Icon size={16} className="text-accent" />
            {prompt?.title}
          </Dialog.Title>
          <div className="px-4 py-3">
            <label htmlFor={inputId} className="mb-1 block text-[12px] text-dim">
              {prompt?.label}
            </label>
            {undoClass !== undefined ? <p className="mb-2 text-sm text-dim">{t(undoClass === null ? "undo.checking" : undoClass === "irreversible" ? "undo.irreversible" : "undo.conditional")}</p> : prompt?.hint && <p className="mb-2 text-sm text-dim">{prompt.hint}</p>}
            <input
              id={inputId}
              ref={inputRef}
              type={prompt?.password ? "password" : "text"}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => {
                // Consume both keys here (TICKET-014): `submit()`/`closeDialogs()`
                // unmount this input synchronously, so without stopping the
                // event the global pane handler would act on it afterwards.
                if (e.key === "Enter") {
                  consumeDialogKey(e);
                  submit();
                } else if (e.key === "Escape") {
                  consumeDialogKey(e);
                  closeDialogs();
                }
              }}
              spellCheck={false}
              autoComplete="off"
              className="w-full rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[13px] text-text outline-none focus:border-accent"
            />
            {prompt?.checkbox && (
              <label className="mt-2.5 flex cursor-pointer items-center gap-2 text-[12px] text-text">
                <input
                  type="checkbox"
                  checked={checked}
                  onChange={(e) => setChecked(e.target.checked)}
                  className="accent-accent"
                />
                {prompt.checkbox.label}
              </label>
            )}
          </div>
          <div className="flex justify-end gap-2 border-t border-edge px-4 py-2.5">
            <button
              onClick={closeDialogs}
              className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
            >
              {t("op.cancel")}
            </button>
            <button
              onClick={submit}
              disabled={!canSubmit}
              onPointerDown={(e) => {
                if (!canSubmit) {
                  e.preventDefault();
                  inputRef.current?.focus();
                }
              }}
              className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
            >
              {prompt?.confirmLabel ?? t("op.create")}
            </button>
          </div>
    </SimpleDialog>
  );
}

// ---------- Progress ----------

// Short, indeterminate display for synchronous operations (delete/folder).
// Copy/move use their own, non-blocking transfer view.
function ProgressOverlay() {
  const busy = useOps((s) => s.busy);
  const t = useT();
  // Hand-rolled before: an ordinary `fixed inset-0` div that covered the
  // workspace visually while leaving every background control in the Tab
  // order (ticket 348). It is a blocking dialog like its siblings, so it
  // gets their chrome — Radix's modal mode is what traps focus. Nothing
  // here is dismissible: the operation is synchronous and has no cancel.
  return (
    <SimpleDialog
      open={busy}
      onOpenChange={() => {}}
      overlayZ="z-[70]"
      contentZ="z-[71]"
      className="w-[280px] p-4"
      noDescription
    >
      {/* `asChild` keeps the live region the screen reader already had
          while giving Radix the title it needs to label the dialog. */}
      <Dialog.Title asChild>
        <div role="status" className="mb-3 text-[13px] text-text">{t("op.progress")}</div>
      </Dialog.Title>
      <div role="progressbar" aria-label={t("op.progress")} className="h-1.5 overflow-hidden rounded bg-panel-inactive">
        <div className="h-full w-1/3 animate-pulse rounded bg-accent" />
      </div>
    </SimpleDialog>
  );
}

// ---------- Closing the window while transfers are running (TICKET-004) ----------

function ForceCloseDialog() {
  const open = useOps((s) => s.forceClose);
  const setForceClose = useOps((s) => s.setForceClose);
  const t = useT();

  const destroyWindow = async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().destroy();
  };

  const cancelAllAndClose = async () => {
    const active = useTransfers.getState().transfers.filter((tr) => !tr.done);
    active.forEach((tr) => cancelTransfer(tr.id));
    setForceClose(false);
    await destroyWindow();
  };

  const closeAnyway = async () => {
    setForceClose(false);
    await destroyWindow();
  };

  return (
    <SimpleDialog
      open={open}
      onOpenChange={(o) => !o && setForceClose(false)}
      overlayZ="z-[80]"
      contentZ="z-[81]"
      className="w-[420px]"
    >
          <Dialog.Title className="flex items-center gap-2 border-b border-edge px-4 py-2.5 text-text">
            <AlertTriangle size={16} className="text-amber-400" />
            {t("op.forceClose.title")}
          </Dialog.Title>
          <Dialog.Description className="px-4 py-3 text-[13px] text-text">
            {t("op.forceClose.message")}
          </Dialog.Description>
          <div className="flex flex-wrap justify-end gap-2 border-t border-edge px-4 py-2.5">
            <button
              onClick={() => setForceClose(false)}
              className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
            >
              {t("op.forceClose.wait")}
            </button>
            <button
              onClick={() => void cancelAllAndClose()}
              className="flex items-center gap-1 rounded bg-amber-500 px-3 py-1 text-[13px] text-white hover:brightness-110"
            >
              <XCircle size={14} />
              {t("op.forceClose.cancelAndClose")}
            </button>
            <button
              onClick={() => void closeAnyway()}
              className="rounded bg-red-500 px-3 py-1 text-[13px] text-white hover:bg-red-600"
            >
              {t("op.forceClose.closeAnyway")}
            </button>
          </div>
    </SimpleDialog>
  );
}
