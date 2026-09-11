# TICKET-060: Ordner über Markieren-Menü und vorhandene Tastenkombinationen auswählen

## Priorität

**P1 – vorgezogen, als nächstes offenes Ticket bearbeiten.** Keine Abhängigkeit
vom Abschluss der weiteren Menüausbauten. Ticketnummer und Dateiname bleiben
für bestehende Verweise erhalten.

## Ziel und korrigierter Umfang

Den Fehler beheben, dass allgemeine Markierungsaktionen im Menü und über ihre
Tastenkombinationen nur Dateien, aber keine Ordner auswählen. Dateien und Ordner
sollen dabei gleich behandelt werden, mit Ausnahme des Navigationseintrags „..“.

Die ursprüngliche Tastenbelegung vor den Änderungswünschen zu diesem Ticket bleibt
vollständig erhalten. **Keine neue F9-, Shift-allein-, Space- oder Enter-Belegung.**
Insbesondere keine Verlagerung von Quick Look auf Enter: ursprüngliches Space-/
Quick-Look-Verhalten, Enter zum Öffnen/Navigieren, Insert, Shift-Bereichsauswahl
und Ctrl/Cmd-Klick unverändert lassen. Vor Umsetzung den tatsächlichen Ausgangsstand
prüfen; die zwischenzeitlichen Belegungsvorschläge waren Ticketplanung.

Die zuvor vorgeschlagenen Änderungen der Auswahlfarben gehören nicht mehr zum
Umfang dieses Fehlertickets. Die gewünschten Git-Schriftfarben erhalten ein
separates Ticket 061. Hier ausschließlich den Auswahlfehler und die zugehörigen
Labels/Tests korrigieren.

## Bestandsprüfung

- `src/features/commander/selection.ts`: allgemeine Auswahlaktionen und Ordnerfilter.
- `src/store/panesStore.ts`: Auswahl pro Tab, Bereichsauswahl und Filter.
- `src/features/commander/useKeyboard.ts`: existierende Zuordnung zu Auswahlaktionen.
- Menü-/Befehlsdefinitionen aus 040/058/059 sowie `FileTable.tsx` vergleichen.
- TICKET-059 sah ausdrücklich nur Dateien für mehrere Sammelaktionen und eine
  Ordner-Opt-in-Regel vor. Diese Einschränkung wird für allgemeine Markierung
  durch den aktuellen Nutzerwunsch aufgehoben. Ursache im aktuellen Code nachweisen.

## Anforderungen

- „Alle markieren“, „Keine markieren“, „Auswahl umkehren“, Muster markieren und
  Muster abwählen behandeln sichtbare Ordner genauso wie Dateien; „..“ ausgeschlossen.
- Ordner müssen bei allgemeinen Mustern standardmäßig enthalten sein. Eine bisher
  notwendige Option „Ordner einbeziehen“ darf die gleichberechtigte Auswahl nicht
  mehr verhindern. Beschriftungen passend auf „Einträge“ statt „Dateien“ umstellen.
- Menü und zugehörige bestehende Tastenkombination rufen dieselbe Auswahlaktion
  auf und liefern dieselbe Zielmenge. Keine Änderung der eigentlichen Kürzel.
- Manuelle Auswahl, Insert, Shift-Bereichsauswahl und Ctrl/Cmd-Klick auf Regressionen
  prüfen. Keine neue Maus- oder Tastaturinteraktion einführen.
- Fachlich dateispezifische Aktionen wie „gleiche Erweiterung“ und Datei-Inhaltsvergleich
  bleiben dateispezifisch; keine künstliche Erweiterungslogik für Ordner ergänzen.
- Auswahl gilt für aktiven Tab/Panel und sichtbare Einträge. Bestehende Filter-/
  Navigationsregeln erhalten; keine unsichtbaren Operationsziele erzeugen.
- Gemischte Auswahl muss korrekt bei Kopieren, Verschieben, Löschen, Packen,
  Auswahl merken/wiederherstellen, Auswahl-Dateien und Export ankommen.
- Darstellung der Ordnerauswahl verwendet dieselben vorhandenen Zustände/Farben
  wie Dateiauswahl. Keine neuen Farben oder Änderungen der Git-Klassifizierung.
- Labels/Hilfe de/en anpassen, soweit „Dateien“ nun auch Ordner einschließt.

## Akzeptanzkriterien und Tests

- [ ] Fehler vor Änderung reproduziert und Ursache dokumentiert.
- [ ] Alle allgemeinen Menüaktionen und ihre bestehenden Tastenkombinationen
      markieren/entmarkieren Dateien und Ordner gleich; „..“ bleibt ausgeschlossen.
- [ ] Tests für nur Ordner, nur Dateien, gemischte/ leere Liste, Muster, Umkehrung,
      Filter, linkes/rechtes Panel und mehrere Tabs bestehen.
- [ ] Menü- und Shortcut-Ergebnisse sind identisch; keine doppelte Ausführung.
- [ ] Bestehende Tastenbelegung einschließlich Space/Quick Look, Enter, Insert,
      Shift und Ctrl/Cmd ist unverändert; Eingabe-/Dialog-/Terminalfokus bleibt geschützt.
- [ ] Gemischte Auswahl wird korrekt an Folgeaktionen übergeben; Snapshot/Export
      verlieren keine ausgewählten Ordner.
- [ ] Details, kompakte Liste, Miniaturen und Baum mit Dateiliste global geprüft,
      einschließlich lokal/remote/Archiv, soweit bereits unterstützt.
- [ ] Keine Änderungen an Auswahlfarben oder Git-Farben in diesem Ticket.

## Definition of Done

- Relevante Auswahl-/Routingtests sowie `cargo check` in `src-tauri`,
  `npm run typecheck` und `npm run build` bestehen.
- `Concept.md`/Menüinventar bei Bedarf korrigieren; abweichende allgemeine
  Auswahlsemantik gegenüber dem erledigten Ticket 059 dokumentieren.
- Erst nach Umsetzung nach `tickets/done/060-unified-selection-colors-and-toggle.done.md`
  verschieben. Lokaler Commit nach Ticketabschluss, ohne Push.
