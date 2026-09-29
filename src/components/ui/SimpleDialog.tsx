// ============================================================
// The chrome every blocking dialog shares (ticket 348).
// ============================================================

import * as Dialog from "@radix-ui/react-dialog";
import { useDialogFocusReturn } from "./useDialogFocusReturn";
import { cn } from "@/lib/cn";
import { markDialogKey } from "@/lib/keys";

/**
 * The chrome the blocking dialogs share: a modal
 * `Dialog.Root`, its overlay, and a centered `Dialog.Content` that returns
 * focus to wherever it came from. The fixed-size sibling of `AppDialog`
 * (no resizing, no stored size). Everything here used to be copied per
 * dialog, which is how `ProgressOverlay` ended up without a focus trap at
 * all — a chrome fix now lands in one place (ticket 348).
 */
export function SimpleDialog({
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
