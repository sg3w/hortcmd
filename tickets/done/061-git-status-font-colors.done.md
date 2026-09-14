# TICKET-061: Git-Schriftfarben – neu grün, geändert blau, untracked rot

## Ziel

Git-Zustände in allen vorhandenen Dateiansichten mit den gewünschten Schriftfarben
anzeigen: **neu grün, geändert blau, untracked rot**. Die Bedeutung der Zustände
korrekt unterscheiden; eine bestehende Datei mit staged Änderungen ist nicht neu.

Eigenständiges Farbticket, getrennt vom vorgezogenen Ordnerauswahlfehler in
[TICKET-060](060-unified-selection-colors-and-toggle.done.md). Keine neue Tastenbelegung
und keine Neugestaltung der Auswahlfarben. Nach 060 einplanen.

## Bestandsprüfung

- `src/lib/fileColors.ts`: zentrale Git-Regeln und Theme-Defaults; bisher untracked
  grün, staged violett und modified gelb. Tatsächlichen Stand vor Umsetzung prüfen.
- `src-tauri/src/commands/fs/git.rs`: `classify()` wertet Git-Porcelain-Status aus;
  bisher werden verschiedene reine Indexänderungen als staged zusammengefasst.
- IPC-Typen, Git-Aggregation an Ordnern, Farblegende, Eigenschaften und
  Einstellungs-Vorschauen auf Statusannahmen untersuchen.

## Verbindliche Zuordnung

| Zustand | Bedeutung | Dunkles Theme | Helles Theme |
| --- | --- | --- | --- |
| Neu | Im Index hinzugefügt (`A`), auch danach weiterbearbeitet (`AM`), sofern kein Konflikt/Löschen vorliegt | Grün `#4ADE80` | Grün `#166534` |
| Geändert | Bestehende Datei geändert, staged oder unstaged (`M `, ` M`, `MM`) | Blau `#60A5FA` | Blau `#1D4ED8` |
| Untracked | Noch nicht durch Git verfolgt (`??`) | Rot `#F87171` | Rot `#B91C1C` |

Farbtöne als zentrale Defaults verwenden und auf Kontrast im jeweiligen Theme
prüfen. Bei notwendigen Kontrastkorrekturen die gewünschte Farbfamilie erhalten.

## Anforderungen

- Neu gegenüber geändert im Rust-Backend bestimmen; keine Git-Semantik im Frontend
  aus Dateinamen oder UI-Zuständen ableiten. Vorhandenes Statusmodell gezielt erweitern,
  falls die bisherige staged-Kategorie die Unterscheidung nicht ausdrücken kann.
- Konflikt, gelöscht, umbenannt, ignoriert und Submodule behalten ihre eigenständige
  Bedeutung. Priorität gegenüber hinzugefügt/geändert ausdrücklich definieren und
  testen. Nicht alle staged Einträge pauschal grün färben.
- Bestehende Aggregationsregeln für Git-Zustände an Ordnern prüfen; neue Zustände
  müssen dort deterministisch einfließen. Ein Ordner mit geändertem Inhalt ist
  nicht allein dadurch ein neu hinzugefügter Ordner.
- Farben ausschließlich über zentrale Registry/CSS-Variablen bereitstellen.
  Details, kompakte Liste, Miniaturen und Baum mit Dateiliste verwenden dieselbe
  Zuordnung; keine hart codierten abweichenden Textfarben in Kindkomponenten.
- Bestehende Auswahlfarben und deren Vorrang beibehalten: Markierung darf nicht
  durch asynchronen Git-Refresh überschrieben werden. Nach Abwahl erscheint der
  aktuelle Git-/Dateifarbzustand wieder. Cursor und Auswahl nicht vermischen.
- Benutzerdefinierte Farbüberschreibungen erhalten. Neue Defaults gelten für nicht
  überschriebene Werte; persistierte Defaults nur gezielt migrieren, falls nötig.
  Bei Aufspaltung von staged dokumentieren, wie bestehende Overrides zugeordnet werden.
- Farbkonfiguration, „Standard wiederherstellen“, Vorschau, Eigenschaften-Legende
  und de/en-Bezeichnungen aktualisieren. Neu nicht als Synonym für untracked erklären.
- Git deaktiviert, kein Repository oder nicht unterstützter Remote-Kontext verwendet
  weiterhin die normale Dateifarbzuordnung. Keine zusätzlichen synchronen Git-Aufrufe
  pro Zeile oder Menüöffnung einführen.

## Akzeptanzkriterien und Tests

- [x] Neue Dateien grün, bestehende geänderte Dateien blau, untracked rot in beiden Themes.
- [x] Rust-Tests decken `A `, `AM`, ` M`, `M `, `MM`, `??`, Konflikt, Löschen,
      Rename, Ignorieren und relevante Submodule-/Ordneraggregation ab.
- [x] Ein reales temporäres Git-Repository bestätigt untracked → hinzugefügt sowie
      Änderung einer bestehenden Datei vor/nach Staging ohne falsche Neu-Klassifizierung.
- [x] UI-/Registrytests prüfen neue Defaults, Themewechsel und Overrides.
- [x] Markieren → Git-Refresh → Abwählen erhält Auswahlfarben und zeigt danach
      den richtigen Git-Zustand; aktives/inaktives Panel und alle Ansichten geprüft.
- [x] Legende, Vorschau, Reset und de/en entsprechen der neuen Bedeutung.
- [x] Keine Änderung der Tastenbelegung oder der Auswahlfarb-Defaults.

## Definition of Done

- Relevante Rust-/Frontendtests, `cargo check` in `src-tauri`,
  `npm run typecheck` und `npm run build` bestehen.
- Bei IPC-Typänderungen `npm run gen:types` ausführen und generierte Bindings prüfen.
- Git-Farbzuordnung und etwaige Migration in `Concept.md` dokumentieren.
- Erst nach Umsetzung nach `tickets/done/061-git-status-font-colors.done.md`
  verschieben. Lokaler Commit nach Ticketabschluss, ohne Push.

## Abschluss

Backend-Klassifikation und zentrale Theme-Defaults umgesetzt. Der persistierte
Slot `git.staged` bleibt für bestehende Overrides erhalten und trifft jetzt nur
`added`; bestehende geänderte Dateien verwenden `git.modified`. Kein Umschreiben
von Einstellungen. Legende, Vorschau und Reset beziehen Werte aus der Registry.

Tests: Porcelain-Matrix einschließlich Prioritäten und Ordneraggregation; echtes
temporäres Git-Repository für untracked → added und bestehende Dateien vor/nach
Staging; Registry-Defaults, Overrides und Themewechsel; Markieren/Refresh/Abwählen
in Details, Liste, Miniaturen und Baum-Dateiliste bei aktivem/inaktivem Panel.

Kontrast gegen aktive/inaktive Panel- und alternierende Zeilenhintergründe:
Minimum dunkel 4,64:1, hell 5,83:1 für die drei neuen Defaults.
