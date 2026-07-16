// ============================================================
// Dialoge & Fortschrittsanzeige für Dateioperationen.
// Bestätigung (Löschen/Fehler), Eingabe (Neuer Ordner),
// Fortschritts-Overlay (aus fs-progress-Events gespeist).
// ============================================================

import { useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { AlertTriangle, FileWarning, FolderPlus, XCircle } from "lucide-react";
import { useOps } from "@/store/opsStore";
import { useTransfers } from "@/store/transfersStore";
import { cancelTransfer, resolveCollision } from "@/ipc/client";
import { useT } from "@/store/settingsStore";
import { cn } from "@/lib/cn";

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

// ---------- Namenskonflikt ----------

function CollisionDialog() {
  const head = useOps((s) => s.collisions[0] ?? null);
  const shiftCollision = useOps((s) => s.shiftCollision);
  const t = useT();
  const [applyAll, setApplyAll] = useState(false);

  // Beim Wechsel auf einen neuen Konflikt „Für alle" zurücksetzen.
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
    <Dialog.Root open={!!head} onOpenChange={() => {}}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[62] bg-black/50" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-[63] w-[440px] max-w-[92vw] -translate-x-1/2 -translate-y-1/2 rounded-lg border border-edge bg-panel shadow-2xl">
          <Dialog.Title className="flex items-center gap-2 border-b border-edge px-4 py-2.5 text-text">
            <FileWarning size={16} className="text-amber-400" />
            {t("op.collision.title")}
          </Dialog.Title>
          <div className="px-4 py-3 text-[13px] text-text">
            {t("op.collision.exists").replace("{name}", name)}
          </div>
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
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function basename(path: string): string {
  if (!path) return "";
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

// ---------- Bestätigung ----------

function ConfirmDialog() {
  const confirm = useOps((s) => s.confirm);
  const closeDialogs = useOps((s) => s.closeDialogs);
  const t = useT();

  const Icon = confirm?.icon ?? (confirm?.danger ? AlertTriangle : null);

  return (
    <Dialog.Root
      open={!!confirm}
      onOpenChange={(o) => !o && closeDialogs()}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[60] bg-black/50" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-[61] w-[420px] max-w-[92vw] -translate-x-1/2 -translate-y-1/2 rounded-lg border border-edge bg-panel shadow-2xl">
          <Dialog.Title className="flex items-center gap-2 border-b border-edge px-4 py-2.5 text-text">
            {Icon && (
              <Icon
                size={16}
                className={confirm?.danger ? "text-red-400" : "text-accent"}
              />
            )}
            {confirm?.title}
          </Dialog.Title>
          <div className="max-h-[240px] overflow-y-auto whitespace-pre-line px-4 py-3 text-[13px] text-text">
            {confirm?.message}
          </div>
          <div className="flex justify-end gap-2 border-t border-edge px-4 py-2.5">
            <button
              onClick={closeDialogs}
              className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
            >
              {t("op.cancel")}
            </button>
            <button
              autoFocus
              onClick={() => {
                confirm?.onConfirm();
                closeDialogs();
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
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

// ---------- Eingabe (Neuer Ordner) ----------

function PromptDialog() {
  const prompt = useOps((s) => s.prompt);
  const closeDialogs = useOps((s) => s.closeDialogs);
  const t = useT();
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (prompt) {
      setValue(prompt.initial);
      // nach dem Öffnen fokussieren und ggf. den angegebenen Bereich vorwählen.
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

  const submit = () => {
    // Erst schließen, dann ausführen: so kann onSubmit selbst einen Folge-
    // Dialog öffnen (z. B. eine Fehlermeldung), ohne sofort überschrieben zu werden.
    const fn = prompt?.onSubmit;
    closeDialogs();
    fn?.(value);
  };

  const Icon = prompt?.icon ?? FolderPlus;

  return (
    <Dialog.Root open={!!prompt} onOpenChange={(o) => !o && closeDialogs()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[60] bg-black/50" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-[61] w-[420px] max-w-[92vw] -translate-x-1/2 -translate-y-1/2 rounded-lg border border-edge bg-panel shadow-2xl">
          <Dialog.Title className="flex items-center gap-2 border-b border-edge px-4 py-2.5 text-text">
            <Icon size={16} className="text-accent" />
            {prompt?.title}
          </Dialog.Title>
          <div className="px-4 py-3">
            <label className="mb-1 block text-[12px] text-dim">
              {prompt?.label}
            </label>
            <input
              ref={inputRef}
              type={prompt?.password ? "password" : "text"}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") submit();
                if (e.key === "Escape") closeDialogs();
              }}
              spellCheck={false}
              autoComplete="off"
              className="w-full rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[13px] text-text outline-none focus:border-accent"
            />
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
              disabled={!value.trim()}
              className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
            >
              {prompt?.confirmLabel ?? t("op.create")}
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

// ---------- Fortschritt ----------

// Kurze, indeterminate Anzeige für synchrone Operationen (Löschen/Ordner).
// Kopieren/Verschieben nutzen die eigene, nicht-blockierende Transfer-Ansicht.
function ProgressOverlay() {
  const busy = useOps((s) => s.busy);
  const t = useT();
  if (!busy) return null;

  return (
    <div className="fixed inset-0 z-[70] flex items-center justify-center bg-black/40">
      <div className="w-[280px] max-w-[92vw] rounded-lg border border-edge bg-panel p-4 shadow-2xl">
        <div className="mb-3 text-[13px] text-text">{t("op.progress")}</div>
        <div className="h-1.5 overflow-hidden rounded bg-panel-inactive">
          <div className="h-full w-1/3 animate-pulse rounded bg-accent" />
        </div>
      </div>
    </div>
  );
}

// ---------- Fenster schließen bei laufenden Transfers (TICKET-004) ----------

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
    <Dialog.Root open={open} onOpenChange={(o) => !o && setForceClose(false)}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[80] bg-black/50" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-[81] w-[420px] max-w-[92vw] -translate-x-1/2 -translate-y-1/2 rounded-lg border border-edge bg-panel shadow-2xl">
          <Dialog.Title className="flex items-center gap-2 border-b border-edge px-4 py-2.5 text-text">
            <AlertTriangle size={16} className="text-amber-400" />
            {t("op.forceClose.title")}
          </Dialog.Title>
          <div className="px-4 py-3 text-[13px] text-text">
            {t("op.forceClose.message")}
          </div>
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
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
