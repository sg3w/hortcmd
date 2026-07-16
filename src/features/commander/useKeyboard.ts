// ============================================================
// Globales TC-Tastaturmodell. Wirkt auf den aktiven Tab des
// aktiven Fensters. Fokus in Eingabefeldern hat Vorrang.
// ============================================================

import { useEffect } from "react";
import { panelOf, usePanes } from "@/store/panesStore";
import { useSettings } from "@/store/settingsStore";
import { isMacOS } from "@/lib/platform";
import { runAction, type ActionId } from "./actions";
import { runDelete, runQuickLook } from "./fileOps";
import { invertSelection, promptSelect } from "./selection";
import { openEntry } from "./navigate";
import { openProps } from "@/store/propsStore";
import { openSearch } from "@/store/searchStore";
import { joinPath } from "@/lib/path";

const PAGE = 20; // Zeilen pro Seiten-Sprung

const F_KEYS: Record<string, ActionId> = {
  F2: "rename",
  F3: "view",
  // F4 wird separat behandelt (Standard-Öffnen vs. „Im Editor öffnen").
  F5: "copy",
  F6: "move",
  F7: "mkdir",
};

/** Öffnet den Cursor-Eintrag. */
function openCursor(): void {
  const s = usePanes.getState();
  openEntry(s.active, panelOf(s, s.active).cursor);
}

/** „Ein Verzeichnis hoch" (Backspace) – nutzt die ".."-Zeile. */
function goUp(): void {
  const s = usePanes.getState();
  const p = panelOf(s, s.active);
  if (p.entries[0]?.parent) openEntry(s.active, 0);
}

export function useKeyboard(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // In Textfeldern (Kommandozeile) nichts abfangen.
      const el = document.activeElement;
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
        return;
      }
      // Bei offenem modalen Dialog (TICKET-012) keine Tastenkürzel an die
      // Dateifenster durchreichen — Dialoge laufen jetzt im selben Fenster
      // und teilen sich sonst denselben globalen Listener.
      if (el instanceof Element && el.closest('[role="dialog"]')) {
        return;
      }

      const s = usePanes.getState();
      const side = s.active;
      const mod = e.metaKey || e.ctrlKey;
      // Spaltenzahl der aktuellen Ansicht (Detail = 1, Grid > 1).
      const cols = Math.max(1, s[side].gridCols);

      // Alt+←/→: im Ordner-Verlauf zurück/vor (wie im Browser).
      if (e.altKey && e.key === "ArrowLeft") {
        e.preventDefault();
        return s.goBack(side);
      }
      if (e.altKey && e.key === "ArrowRight") {
        e.preventDefault();
        return s.goForward(side);
      }

      // Alt+Enter: Eigenschaften/Rechte des Cursor-Eintrags (nicht im Archiv).
      if (e.altKey && e.key === "Enter") {
        e.preventDefault();
        const p = panelOf(s, side);
        const cur = p.entries[p.cursor];
        if (cur && !cur.parent && p.archive === null) {
          openProps(joinPath(p.path, cur.name));
        }
        return;
      }

      // ----- Zwischenablage & Tabs (Cmd/Ctrl-Kürzel) -----
      if (mod) {
        switch (e.key.toLowerCase()) {
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
        }
      }

      // ----- Schnellfilter (Filtern beim Tippen) -----
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
        // Druckbare Zeichen: bei aktivem Filter anhängen; sonst mit
        // Buchstabe/Ziffer/._ starten (+,-,* bleiben Muster-Auswahl, Space bleibt frei).
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
          // Standard: Papierkorb; Shift erzwingt endgültiges Löschen.
          e.preventDefault();
          runDelete(side, { permanent: e.shiftKey });
          return;
        case "Insert":
          e.preventDefault();
          s.toggleMark(side, panelOf(s, side).cursor, true);
          return;
        case " ":
          // macOS: Leertaste öffnet Quick Look; sonst (oder Archiv/„..“) markieren.
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

      // Alt+F5: Auswahl packen (TC-Kürzel).
      if (e.altKey && e.key === "F5") {
        e.preventDefault();
        runAction("pack", side);
        return;
      }

      // Alt+F7: Suche öffnen (TC-Kürzel).
      if (e.altKey && e.key === "F7") {
        e.preventDefault();
        openSearch(side);
        return;
      }

      // F4 / Umschalt+F4: „Im Editor öffnen" je nach Einstellung.
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
