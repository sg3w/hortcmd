# TICKET-063: Ordner per Button im gegenüberliegenden Panel öffnen

## Ziel

Ein Button zeigt den gewählten Ordner auch auf der anderen Seite an, sodass beide
Panels denselben Ordner nutzen können. Gemeint ist das Duplizieren der Ansicht:
keine Dateien kopieren, keinen zweiten Ordner auf dem Datenträger anlegen.

## Verhalten

- Toolbar-Button **„Ordner auf anderer Seite öffnen“** für das aktive Panel.
  Standardziel ist dessen aktuell angezeigter Ordner. Damit lässt sich unmittelbar
  links/rechts derselbe Ordner anzeigen.
- Zusätzlich denselben Befehl im Ordner-Kontextmenü anbieten: dort ist der konkret
  angeklickte Ordner das Ziel. Dadurch ist auch ein gewählter Unterordner direkt
  auf der Gegenseite erreichbar. Die Quelle wird dabei nicht automatisch navigiert.
- Toolbar und Kontextmenü müssen ihren Zielbezug durch Tooltip/Label deutlich
  machen: „Aktuellen Ordner auf anderer Seite öffnen“ bzw. „Auf anderer Seite öffnen“.
- Ziel ist der aktive Tab des gegenüberliegenden Panels; dessen sonstige Tabs
  bleiben bestehen. Kein neuer Tab und kein Umschalten der aktiven Quellseite.
- Bestehende Navigation samt Verlauf, Aktualisierung, Auswahlbereinigung und
  Session-Speicherung verwenden. Ansicht, Sortierung und Filter der Panels nicht
  ungeprüft klonen; vorhandene Navigationsregeln gelten.
- Für lokale Ordner, verbundene Remote-Adressen und unterstützte Archiv-Innenpfade
  die vollständige bestehende Ortsidentität übernehmen. Keine Remote-URI als
  lokalen Dateipfad behandeln und bei Archiven Container und Innenpfad erhalten.
- Bei fehlender Verbindung, nicht zugänglichem Ziel oder nicht unterstütztem
  Kontext verständlichen Fehler anzeigen; Quellpanel bleibt unverändert.
- Keine dauerhafte Kopplung: spätere Navigation in einem Panel ist unabhängig.

## Umsetzung

- Vorhandene Toolbar `src/features/commander/CommandBar.tsx` erweitern; ein passendes
  Richtung-/Panel-Icon mit lokalisiertem de/en-Tooltip und zugänglichem Namen nutzen.
- Gemeinsame Aktion mit expliziter Quellseite und Zieladresse; Navigation über
  `src/store/panesStore.ts` (`loadDir`/`loadArchive`) und bestehendes Pfadmodell.
- Kontextmenü und gegebenenfalls **Befehle → Aktuellen Ordner auf anderer Seite
  öffnen** verwenden dieselbe Aktion. In Menüinventar aus 058 aufnehmen.
- Bestehende Tastenbelegung unverändert lassen; kein neues Kürzel erforderlich.

## Akzeptanzkriterien und Tests

- [ ] Toolbar-Button links → rechts und rechts → links zeigt den aktuellen
      Quellordner auf der Gegenseite; beide Panels zeigen danach denselben Ort.
- [ ] Kontextmenü eines Unterordners öffnet genau diesen Ordner auf der Gegenseite.
- [ ] Aktive Quellseite und andere Tabs bleiben erhalten; Zielnavigation funktioniert
      mit bestehendem Zurück-Verlauf und erzeugt keine Dateioperation.
- [ ] Lokaler Ordner, Wurzelverzeichnis, Unicode-/Leerzeichenpfad, Remote-Ort und
      unterstützter Archiv-Innenpfad sind geprüft.
- [ ] Fehler/Disconnect führen zu verständlicher Rückmeldung ohne Quellnavigation.
- [ ] Wiederholter Klick, bereits gleicher Zielort und leeres Verzeichnis funktionieren.
- [ ] Tooltip, Tastaturfokus, de/en und beide Themes geprüft.

## Definition of Done

- Gezielte Routing-/Navigationstests und manuelle Prüfung beider Richtungen bestehen.
- `cargo check` in `src-tauri`, `npm run typecheck` und `npm run build` erfolgreich.
- `Concept.md` und Menüinventar aktualisieren.
- Erst nach Umsetzung nach `tickets/done/063-open-folder-in-opposite-pane.done.md`
  verschieben. Lokaler Commit nach Ticketabschluss, ohne Push.
