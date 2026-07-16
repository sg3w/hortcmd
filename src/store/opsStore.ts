// ============================================================
// Zustand für laufende Dateioperationen: Fortschrittsanzeige
// sowie Bestätigungs- und Eingabedialoge.
// ============================================================

import { create } from "zustand";
import type { LucideIcon } from "lucide-react";
import type { CollisionReq, Preview } from "@/ipc/bindings";

export interface ConfirmRequest {
  title: string;
  message: string;
  danger?: boolean;
  /** Beschriftung des Bestätigen-Buttons (Standard: „OK"). */
  confirmLabel?: string;
  /** Symbol im Dialogkopf (Standard: Warndreieck bei danger). */
  icon?: LucideIcon;
  onConfirm: () => void;
}

export interface PromptRequest {
  title: string;
  label: string;
  initial: string;
  confirmLabel?: string;
  /** Symbol im Dialogkopf (Standard: Ordner-Plus). */
  icon?: LucideIcon;
  /** Textbereich, der beim Öffnen vorausgewählt wird (z. B. Name ohne Endung). */
  selectRange?: [number, number];
  /** Eingabe maskieren (Passwortfeld). */
  password?: boolean;
  onSubmit: (value: string) => void;
}

interface OpsStore {
  busy: boolean;
  confirm: ConfirmRequest | null;
  prompt: PromptRequest | null;
  collisions: CollisionReq[]; // Warteschlange offener Namenskonflikte
  preview: Preview | null;
  /** Fenster-Schließen wurde bei laufenden Transfers abgefangen (TICKET-004). */
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
