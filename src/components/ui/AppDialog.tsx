// ============================================================
// Central, reusable dialog management (TICKET-012).
//
// Replaces the former management of separate Tauri windows: dialogs
// are rendered as React components inside the main window (Radix
// Dialog), always opened centered, can be resized via a drag handle at
// the bottom right (bounds: a per-dialog minimum up to 90 % of the main
// window size) and remember their last used size per dialog type
// (`dialogKey`) across restarts (not the position — that is always
// re-centered).
//
// Modality, focus trap/restore and `Esc` to close come from Radix'
// `Dialog.Root` (default `modal={true}`) — non-modal dialogs would have
// to opt out explicitly with `modal={false}`, which no dialog of this
// app currently needs.
// ============================================================

import type { MouseEvent as ReactMouseEvent, ReactNode } from "react";
import { useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useDialogSizeStore, type DialogSize } from "@/store/dialogSizeStore";

interface Props {
  /** Stable identifier of the dialog type — key for size persistence. */
  dialogKey: string;
  open: boolean;
  onClose: () => void;
  /** Content of the title bar (icon + text + possibly badges). */
  titleBar: ReactNode;
  /** Optional footer (buttons, counters …). */
  footer?: ReactNode;
  defaultSize: DialogSize;
  /** Minimum size; default = `defaultSize`. */
  minSize?: DialogSize;
  /** No drag handle — fixed size (e.g. when the content cannot be
   *  scaled sensibly). Default: resizable. */
  fixedSize?: boolean;
  children: ReactNode;
}

const MAX_FRAC = 0.9; // at most 90 % of the main window size

function clamp(size: DialogSize, min: DialogSize): DialogSize {
  const maxW = Math.max(min.w, window.innerWidth * MAX_FRAC);
  const maxH = Math.max(min.h, window.innerHeight * MAX_FRAC);
  return {
    w: Math.min(maxW, Math.max(min.w, size.w)),
    h: Math.min(maxH, Math.max(min.h, size.h)),
  };
}

function ResizeHandle({ onResize }: { onResize: (e: ReactMouseEvent) => void }) {
  return (
    <div
      onMouseDown={onResize}
      className="absolute bottom-0 right-0 z-10 flex h-4 w-4 cursor-nwse-resize items-end justify-end"
    >
      <svg width="10" height="10" viewBox="0 0 10 10" className="text-dim">
        <g stroke="currentColor" strokeWidth="1" strokeLinecap="round" opacity="0.6">
          <line x1="9" y1="2" x2="2" y2="9" />
          <line x1="9" y1="5" x2="5" y2="9" />
          <line x1="9" y1="8" x2="8" y2="9" />
        </g>
      </svg>
    </div>
  );
}

export function AppDialog({
  dialogKey,
  open,
  onClose,
  titleBar,
  footer,
  defaultSize,
  minSize,
  fixedSize,
  children,
}: Props) {
  const min = minSize ?? defaultSize;
  // Deliberately no reactive store hook (`useDialogSizeStore(selector)`) here:
  // with several `AppDialog` instances mounted at the same time (all dialogs
  // are unconditionally mounted in App.tsx, only `open` distinguishes them)
  // the persist rehydration would otherwise trigger a "setState while
  // rendering a different component" warning — even with a stable actions
  // selector, since merely notifying all subscribers on rehydration counts.
  // `size` is kept locally via `useState`; the store only serves as a plain
  // read/write depot accessed through `.getState()`.
  const [size, setSize] = useState<DialogSize>(() =>
    clamp(useDialogSizeStore.getState().sizes[dialogKey] ?? defaultSize, min),
  );

  // On open: adopt the stored size (clamped to main window/minimum) — a main
  // window that has become too small resets the dialog fully back into the
  // visible area.
  useEffect(() => {
    if (!open) return;
    const stored = useDialogSizeStore.getState().sizes[dialogKey];
    setSize(clamp(stored ?? defaultSize, min));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // Main window is shrunk in the meantime: keep the dialog within the
  // visible area.
  useEffect(() => {
    if (!open) return;
    const onWindowResize = () => setSize((s) => clamp(s, min));
    window.addEventListener("resize", onWindowResize);
    return () => window.removeEventListener("resize", onWindowResize);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // Drag handle at the bottom right. Since the dialog is centered
  // (translate -50 %), it grows by twice the mouse movement so that the
  // corner follows the cursor. The size is persisted only on release.
  const startResize = (e: ReactMouseEvent) => {
    e.preventDefault();
    const sx = e.clientX;
    const sy = e.clientY;
    const sw = size.w;
    const sh = size.h;
    const onMove = (ev: MouseEvent) => {
      setSize(
        clamp({ w: sw + (ev.clientX - sx) * 2, h: sh + (ev.clientY - sy) * 2 }, min),
      );
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      setSize((current) => {
        useDialogSizeStore.getState().setSize(dialogKey, current);
        return current;
      });
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/50" />
        <Dialog.Content
          style={{ width: size.w, height: size.h }}
          className="fixed left-1/2 top-1/2 z-50 flex max-h-[90vh] max-w-[90vw] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-edge bg-panel shadow-2xl"
          aria-describedby={undefined}
        >
          <Dialog.Title asChild>
            <div className="flex flex-shrink-0 items-center gap-2 border-b border-edge bg-header px-4 py-2 text-text">
              {titleBar}
            </div>
          </Dialog.Title>
          {children}
          {footer}
          {!fixedSize && <ResizeHandle onResize={startResize} />}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
