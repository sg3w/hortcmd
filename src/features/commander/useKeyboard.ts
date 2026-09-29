// ============================================================
// Global TC keyboard model. Acts on the active tab of the
// active pane. Focus in input fields takes precedence.
// ============================================================

import { useEffect } from "react";
import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useSettings } from "@/store/settingsStore";
import { isMacOS } from "@/lib/platform";
import { runAction, type ActionId } from "./actions";
import { runDelete, runQuickLook } from "./fileOps";
import {
  invertSelection,
  promptSelect,
  restoreSelection,
  selectAllFiles,
  selectNoFiles,
  selectSameExt,
} from "./selection";
import { runCompare } from "./compareLevel";
import { guard } from "./guard";
import { openEntry } from "./navigate";
import { openProps } from "@/store/propsStore";
import { openUndo, undoAvailable } from "@/store/undoStore";
import { openSearch } from "@/store/searchStore";
import { openSettings } from "@/store/uiStore";
import { joinPath } from "@/lib/path";
import { isDialogKey } from "@/lib/keys";

const PAGE = 20; // Zeilen pro Seiten-Sprung

/**
 * Numeric-keypad selection keys (TICKET-059), addressed by `code` and
 * not by `key`: the keypad reports the same `key` as the main block, so
 * only `code` tells "Num +" and "+" apart. The plain keys of the main
 * block keep working unchanged; the keypad merely adds the modifier
 * combinations Norton Commander users expect. Every one of them is also
 * reachable from the "Mark" menu, so a keyboard without a keypad loses
 * nothing.
 */
function numpadSelection(e: KeyboardEvent, side: Side): boolean {
  const mod = e.metaKey || e.ctrlKey;
  switch (e.code) {
    case "NumpadAdd":
      if (mod) selectAllFiles(side);
      else if (e.altKey) selectSameExt(side);
      else promptSelect(side, true);
      return true;
    case "NumpadSubtract":
      if (e.altKey) return false;
      mod ? selectNoFiles(side) : promptSelect(side, false);
      return true;
    case "NumpadMultiply":
      if (mod || e.altKey) return false;
      invertSelection(side);
      return true;
    case "NumpadDivide":
      if (mod || e.altKey) return false;
      restoreSelection(side);
      return true;
    default:
      return false;
  }
}

const F_KEYS: Record<string, ActionId> = {
  F2: "rename",
  F3: "view",
  // F4 is handled separately (default open vs. "Open in editor").
  F5: "copy",
  F6: "move",
  F7: "mkdir",
};

/** Opens the cursor entry. */
function openCursor(): void {
  const s = usePanes.getState();
  openEntry(s.active, panelOf(s, s.active).cursor);
}

/** "Up one directory" (Backspace) – uses the ".." row. */
function goUp(): void {
  const s = usePanes.getState();
  const p = panelOf(s, s.active);
  if (p.entries[0]?.parent) openEntry(s.active, 0);
}

export function useKeyboard(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // A dialog already handled this keystroke (TICKET-014). Checked first
      // and on the event itself, because a dialog that closes on `Enter`/
      // `Escape` unmounts while this very event is still being dispatched —
      // afterwards neither `document.activeElement` nor the DOM tell us where
      // the keystroke came from.
      if (isDialogKey(e) || e.defaultPrevented) return;

      // Don't intercept anything in text fields (command line). Read the
      // event target rather than `document.activeElement` for the same reason:
      // focus has moved on once the field is gone.
      const el = e.target instanceof Element ? e.target : document.activeElement;
      if ((e.key === "Enter" || e.key === " ") && el instanceof Element &&
          el.closest('button,select,[role="button"],[role="combobox"],[role="tab"]')) return;
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
        return;
      }
      if (el instanceof HTMLElement && el.isContentEditable) {
        return;
      }
      // With a modal dialog open (TICKET-012), don't pass shortcuts through to
      // the file panes — dialogs now run in the same window
      // and would otherwise share the same global listener.
      if (el instanceof Element && el.closest('[role="dialog"]')) {
        return;
      }
      // Same guard for keystrokes that never reached the dialog itself: it may
      // be open but not focused yet (the prompt input focuses one frame later).
      if (document.querySelector('[role="dialog"]')) {
        return;
      }
      // An open menu (application menu, context menu, dropdown) owns the
      // keyboard: arrows navigate it and printable characters drive its
      // type-ahead. Without this guard they would additionally fill the
      // quick filter or run a file action behind the menu (TICKET-040).
      if (el instanceof Element && el.closest('[role="menu"],[role="menubar"]')) {
        return;
      }
      if (document.querySelector('[role="menu"][data-state="open"]')) {
        return;
      }

      // Tab follows the browser's focus order once a control owns focus.
      // The legacy pane switch applies only to an unfocused workspace.
      if (e.key === "Tab" && el && el !== document.body && el !== document.documentElement) return;

      const s = usePanes.getState();
      const side = s.active;
      const mod = e.metaKey || e.ctrlKey;
      // Beta and off by default (TICKET-391): while it is off the key is
      // not ours, so it is not swallowed either.
      if (mod && e.altKey && e.key.toLowerCase() === "z" && undoAvailable()) { e.preventDefault(); openUndo(); return; }
      // Column count of the current view (detail = 1, grid > 1).
      const cols = Math.max(1, s[side].gridCols);

      // Alt+←/→: back/forward in the folder history (like in a browser).
      if (e.altKey && e.key === "ArrowLeft") {
        e.preventDefault();
        return s.goBack(side);
      }
      if (e.altKey && e.key === "ArrowRight") {
        e.preventDefault();
        return s.goForward(side);
      }

      // Alt+Enter: properties/permissions of the cursor entry (not in an archive).
      if (e.altKey && e.key === "Enter") {
        e.preventDefault();
        const p = panelOf(s, side);
        const cur = p.entries[p.cursor];
        if (!cur || cur.parent || p.archive !== null) return;
        // A shortcut for something this location cannot do says so
        // rather than doing nothing (TICKET-031).
        if (!guard(side, p.path, "stat")) return;
        openProps(joinPath(p.path, cur.name), p.git?.entries[cur.name]);
        return;
      }

      // ----- Selection keys of the numeric keypad (TICKET-059) -----
      // Before the Cmd/Ctrl and quick-filter blocks: Cmd+Num+ must
      // select everything, not fall through to a clipboard shortcut.
      if (numpadSelection(e, side)) {
        e.preventDefault();
        return;
      }

      // Shift+F2: compare the two panes. Modifier combinations are
      // decided before the plain F-keys further down, so this cannot
      // end up in "rename" (F2).
      if (e.shiftKey && !mod && !e.altKey && e.key === "F2") {
        e.preventDefault();
        void runCompare("differences");
        return;
      }

      // ----- Clipboard & tabs (Cmd/Ctrl shortcuts) -----
      if (mod) {
        switch (e.key.toLowerCase()) {
          case "+":
          case "=":
          case "-":
          case "0":
            // Leave main-keyboard zoom shortcuts to the host. Numeric
            // keypad selection shortcuts have already been handled above.
            return;
          case "c":
            e.preventDefault();
            return runAction("clip-copy", side);
          case "x":
            e.preventDefault();
            return runAction("clip-cut", side);
          case "v":
            e.preventDefault();
            return runAction("clip-paste", side);
          case "t":
            e.preventDefault();
            return void s.addTab(side);
          case "w":
            e.preventDefault();
            return s.closeTab(side, s[side].activeTab);
          case ",":
            // Settings (TICKET-040). On macOS the native app menu owns
            // this accelerator; here it covers Windows/Linux as well.
            e.preventDefault();
            return openSettings();
        }
      }

      // ----- Quick filter (filter while typing) -----
      const panel = panelOf(s, side);
      const filterActive = panel.filter.length > 0;
      if (!mod && !e.altKey) {
        if (e.key === "Escape" && filterActive) {
          e.preventDefault();
          return s.setFilter(side, "");
        }
        if (filterActive && e.key === "Backspace") {
          e.preventDefault();
          return s.setFilter(side, panel.filter.slice(0, -1));
        }
        // Printable characters: append when the filter is active; otherwise start
        // with letter/digit/._ (+,-,* stay pattern selection, Space stays free).
        if (e.key.length === 1 && e.key !== " ") {
          if (filterActive) {
            e.preventDefault();
            return s.setFilter(side, panel.filter + e.key);
          }
          if (/[\p{L}\p{N}._]/u.test(e.key)) {
            e.preventDefault();
            return s.setFilter(side, e.key);
          }
        }
      }

      switch (e.key) {
        case "Tab":
          e.preventDefault();
          s.setActive(side === "left" ? "right" : "left");
          document.querySelector<HTMLElement>(`#${side === "left" ? "right" : "left"}-files [role="grid"]`)?.focus();
          return;
        case "ArrowDown":
          e.preventDefault();
          e.shiftKey ? s.shiftMove(side, cols) : s.moveCursor(side, cols);
          return;
        case "ArrowUp":
          e.preventDefault();
          e.shiftKey ? s.shiftMove(side, -cols) : s.moveCursor(side, -cols);
          return;
        case "ArrowRight":
          if (cols === 1) break; // Detailansicht: nichts abfangen
          e.preventDefault();
          e.shiftKey ? s.shiftMove(side, 1) : s.moveCursor(side, 1);
          return;
        case "ArrowLeft":
          if (cols === 1) break;
          e.preventDefault();
          e.shiftKey ? s.shiftMove(side, -1) : s.moveCursor(side, -1);
          return;
        case "PageDown":
          e.preventDefault();
          e.shiftKey ? s.shiftMove(side, PAGE) : s.moveCursor(side, PAGE);
          return;
        case "PageUp":
          e.preventDefault();
          e.shiftKey ? s.shiftMove(side, -PAGE) : s.moveCursor(side, -PAGE);
          return;
        case "Home":
          e.preventDefault();
          s.jumpCursor(side, "top");
          return;
        case "End":
          e.preventDefault();
          s.jumpCursor(side, "bottom");
          return;
        case "Enter":
          e.preventDefault();
          openCursor();
          return;
        case "Backspace":
          e.preventDefault();
          goUp();
          return;
        case "F8":
        case "Delete":
          // Default: trash; Shift forces permanent deletion.
          e.preventDefault();
          runDelete(side, { permanent: e.shiftKey });
          return;
        case "Insert":
          e.preventDefault();
          s.toggleMark(side, panelOf(s, side).cursor, true);
          return;
        case " ":
          // macOS: Space opens Quick Look; otherwise (or archive/"..") select.
          e.preventDefault();
          if (isMacOS && runQuickLook(side)) return;
          s.toggleMark(side, panelOf(s, side).cursor, true);
          return;
        case "*":
          e.preventDefault();
          invertSelection(side);
          return;
        case "+":
          e.preventDefault();
          promptSelect(side, true);
          return;
        case "-":
          e.preventDefault();
          promptSelect(side, false);
          return;
      }

      // Alt+F5: pack the selection (TC shortcut).
      if (e.altKey && e.key === "F5") {
        e.preventDefault();
        runAction("pack", side);
        return;
      }

      // Alt+F7: open search (TC shortcut).
      if (e.altKey && e.key === "F7") {
        e.preventDefault();
        if (!guard(side, panelOf(s, side).path, "search")) return;
        openSearch(side);
        return;
      }

      // F4 / Shift+F4: "Open in editor" depending on the setting.
      if (e.key === "F4") {
        e.preventDefault();
        const trigger = useSettings.getState().editorTrigger;
        const useAssoc =
          trigger === "f4" || (trigger === "shiftF4" && e.shiftKey);
        runAction(useAssoc ? "edit-with" : "edit", side);
        return;
      }

      if (F_KEYS[e.key]) {
        e.preventDefault();
        runAction(F_KEYS[e.key], side);
      }
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
