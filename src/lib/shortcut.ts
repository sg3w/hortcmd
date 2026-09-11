// ============================================================
// Platform-neutral description of a keyboard shortcut and its
// rendering for menus and the shortcut overview (TICKET-040).
//
// A command stores its shortcut as tokens, never as a finished string,
// so the same definition renders as "⌘C" on macOS and as "Ctrl+C" on
// Windows/Linux — and so the shortcut overview in the Help menu can be
// generated from exactly the same source as the menu entries.
// ============================================================

import { isMacOS } from "./platform";

/** `Mod` is Cmd on macOS and Ctrl elsewhere. */
export type Modifier = "Mod" | "Alt" | "Shift";

/** A shortcut: any number of modifiers plus one key. */
export interface Shortcut {
  mods?: Modifier[];
  key: string;
}

const MAC_MOD: Record<Modifier, string> = {
  Mod: "⌘",
  Alt: "⌥",
  Shift: "⇧",
};

const PC_MOD: Record<Modifier, string> = {
  Mod: "Ctrl",
  Alt: "Alt",
  Shift: "Shift",
};

/** Numeric-keypad keys, spelled out so they cannot be confused with
 *  the keys of the main block (TICKET-059). Every command carrying one
 *  is also reachable from the menu, for keyboards without a keypad. */
const NUM_KEY: Record<string, string> = {
  "Num+": "Num +",
  "Num-": "Num −",
  "Num*": "Num *",
  "Num/": "Num /",
};

/** Keys whose macOS glyph differs from their name. */
const MAC_KEY: Record<string, string> = {
  Enter: "⏎",
  Backspace: "⌫",
  Delete: "⌦",
  Space: "␣",
  Left: "←",
  Right: "→",
  Up: "↑",
  Down: "↓",
};

const PC_KEY: Record<string, string> = {
  Left: "←",
  Right: "→",
  Up: "↑",
  Down: "↓",
};

/** Human-readable shortcut for the current platform ("⌘C" / "Ctrl+C"). */
export function formatShortcut(s: Shortcut): string {
  const mods = s.mods ?? [];
  if (isMacOS) {
    // macOS convention: modifier glyphs directly in front of the key,
    // always in the order ⌃⌥⇧⌘ — here only ⌥⇧⌘ occur.
    const order: Modifier[] = ["Alt", "Shift", "Mod"];
    const glyphs = order
      .filter((m) => mods.includes(m))
      .map((m) => MAC_MOD[m])
      .join("");
    return glyphs + (NUM_KEY[s.key] ?? MAC_KEY[s.key] ?? s.key);
  }
  return [...mods.map((m) => PC_MOD[m]), NUM_KEY[s.key] ?? PC_KEY[s.key] ?? s.key].join(
    "+",
  );
}
