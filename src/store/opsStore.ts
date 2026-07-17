// ============================================================
// State for running file operations: progress display
// as well as confirmation and input dialogs.
// ============================================================

import { create } from "zustand";
import type { LucideIcon } from "lucide-react";
import type { CollisionReq, Preview } from "@/ipc/bindings";

export interface ConfirmRequest {
  title: string;
  message: string;
  danger?: boolean;
  /** Label of the confirm button (default: "OK"). */
  confirmLabel?: string;
  /** Icon in the dialog header (default: warning triangle for danger). */
  icon?: LucideIcon;
  onConfirm: () => void;
}

export interface PromptRequest {
  title: string;
  label: string;
  initial: string;
  confirmLabel?: string;
  /** Icon in the dialog header (default: folder plus). */
  icon?: LucideIcon;
  /** Text range preselected on open (e.g. the name without the extension). */
  selectRange?: [number, number];
  /** Mask the input (password field). */
  password?: boolean;
  onSubmit: (value: string) => void;
}

interface OpsStore {
  busy: boolean;
  confirm: ConfirmRequest | null;
  prompt: PromptRequest | null;
  collisions: CollisionReq[]; // Warteschlange offener Namenskonflikte
  preview: Preview | null;
  /** Window closing was intercepted during running transfers (TICKET-004). */
  forceClose: boolean;

  setBusy: (b: boolean) => void;
  requestConfirm: (r: ConfirmRequest) => void;
  requestPrompt: (r: PromptRequest) => void;
  closeDialogs: () => void;
  pushCollision: (c: CollisionReq) => void;
  shiftCollision: () => void;
  setPreview: (p: Preview | null) => void;
  setForceClose: (v: boolean) => void;
}

export const useOps = create<OpsStore>((set) => ({
  busy: false,
  confirm: null,
  prompt: null,
  collisions: [],
  preview: null,
  forceClose: false,

  setBusy: (busy) => set({ busy }),
  requestConfirm: (confirm) => set({ confirm }),
  requestPrompt: (prompt) => set({ prompt }),
  closeDialogs: () => set({ confirm: null, prompt: null }),
  pushCollision: (c) => set((s) => ({ collisions: [...s.collisions, c] })),
  shiftCollision: () => set((s) => ({ collisions: s.collisions.slice(1) })),
  setPreview: (preview) => set({ preview }),
  setForceClose: (forceClose) => set({ forceClose }),
}));
