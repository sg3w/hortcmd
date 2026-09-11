// ============================================================
// Transient per-pane notices (TICKET-059).
//
// Selection restores, selection-file loads and pane comparisons all
// finish with a result the user has to see — "42 restored, 3 gone",
// "clipboard refused the write". None of those warrant a modal, and
// none of them belong in the pane state itself: they are messages
// about an action, not part of the folder.
//
// The text is stored as a key plus variables, never as a finished
// string, so a notice that is still on screen follows a language
// switch instead of freezing in the language it was raised in.
// ============================================================

import { create } from "zustand";
import type { TransKey } from "@/i18n/dictionaries";
import type { Side } from "./panesStore";

export interface Notice {
  key: TransKey;
  /** Replacements for "{name}" placeholders in the translation. */
  vars?: Record<string, string | number>;
  tone: "info" | "error";
  /** Distinguishes two notices with identical text (re-trigger). */
  seq: number;
}

interface NoticeStore {
  left: Notice | null;
  right: Notice | null;
  show: (
    side: Side,
    key: TransKey,
    vars?: Record<string, string | number>,
    tone?: Notice["tone"],
  ) => void;
  clear: (side: Side) => void;
}

let seq = 0;

export const useNotices = create<NoticeStore>((set) => ({
  left: null,
  right: null,
  show: (side, key, vars, tone = "info") =>
    set({ [side]: { key, vars, tone, seq: ++seq } } as Pick<NoticeStore, Side>),
  clear: (side) => set({ [side]: null } as Pick<NoticeStore, Side>),
}));

/** Convenience call from outside React. */
export function notify(
  side: Side,
  key: TransKey,
  vars?: Record<string, string | number>,
  tone: Notice["tone"] = "info",
): void {
  useNotices.getState().show(side, key, vars, tone);
}

/** Fills "{a}" placeholders of an already translated text. */
export function fillVars(
  text: string,
  vars: Record<string, string | number> | undefined,
): string {
  if (!vars) return text;
  return Object.entries(vars).reduce(
    (acc, [k, v]) => acc.split(`{${k}}`).join(String(v)),
    text,
  );
}
