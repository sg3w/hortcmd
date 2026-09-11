# TICKET-060: Markierung vereinheitlichen, Shift-Umschaltung und klare Auswahl-/Git-Farben

## Priorität und Reihenfolge

**P1 – vorgezogen, als nächstes offenes Ticket bearbeiten.** Expliziter Nutzerwunsch:
vor den weiteren Menüausbauten umsetzen. Keine Abhängigkeit vom Abschluss der
Tickets 040/058; vorhandene Auswahl, Ansichten und Tastatur direkt korrigieren.
Bereits laufende Arbeit sauber abschließen, danach dieses Ticket vorziehen.

## Ziel

Dateien und Ordner lassen sich gleich markieren und wieder abwählen. Ein einzelner
Shift-Tastendruck schaltet den Eintrag unter dem Cursor um, ohne weitere Taste.
Markierte Einträge haben einen blauen Hintergrund und deutlich gelbe Schrift.
Nicht markierte Git-Einträge zeigen neue Dateien grün, geänderte blau und
untracked rot. Alle Ansichten und Bedienwege global auf Konsistenz prüfen.

## Bestandsbefund und Abgrenzung

- `FileTable.tsx` schaltet per Cmd/Ctrl-Klick bzw. wählt Bereiche per Shift;
  `useKeyboard.ts` kennt bereits Insert, auf macOS dient Space Quick Look.
- `fileColors.ts` setzt derzeit untracked grün, staged violett, modified gelb;
  `index.css` definiert die Auswahltextfarbe im hellen Theme braun statt gelb.
- Git `classify()` in `src-tauri/src/commands/fs/git.rs` fasst verschiedene
  Indexänderungen als staged zusammen; staged bedeutet nicht automatisch neu.
- Das erledigte Ticket 059 sah für mehrere Sammelaktionen ausdrücklich nur Dateien
  vor. Dieses Ticket **ersetzt diese Einschränkung**: Ordner und Dateien verhalten
  sich bei allgemeinen Markierungsaktionen gleich. Den Fehler global reproduzieren
  und Ursache dokumentieren, nicht nur die Menübeschriftung ändern.

## Anforderungen

### Shift allein zum Markieren und Aufheben

- **Shift allein drücken und loslassen = Markierung umschalten** für den
  Cursor-Eintrag: unmarkiert → markiert, markiert → unmarkiert. Linke und rechte
  Shift-Taste unterstützen. Cursor bleibt auf dem Eintrag; keine Navigation.
- Umschalten erst beim Loslassen auslösen, wenn seit dem Drücken keine andere
  Taste und keine Maus-/Pointeraktion beteiligt war. Shift+Pfeil, Shift+Klick,
  Shift+Funktionstaste und sonstige Kombinationen dürfen keine zusätzliche
  Markierungsumschaltung auslösen. Autorepeat und beide Shift-Tasten zusammen
  dürfen keine doppelten Umschaltungen erzeugen.
- Begann die Geste außerhalb des Dateipanels oder wechselten Fokus, aktiver Tab,
  Cursorziel oder Fensteraktivierung, die ausstehende Umschaltung verwerfen.
  Beim Fokusverlust/Blur Tastenzustand zurücksetzen, damit kein späteres Keyup
  versehentlich markiert.
- In Menü/Hilfe als „Shift (einzeln)“ beschriften und zusätzlich einen klickbaren
  Markieren/Abwählen-Befehl anbieten. F9 erhält durch dieses Ticket keine Belegung.
- Vorhandene Insert-/Mehrfachauswahlkürzel dürfen als kompatible Alternativen bleiben.
  Shift-Bereichsauswahl und Cmd/Ctrl-Klick werden nicht entfernt, sind aber keine
  Voraussetzung mehr. Quick Look auf macOS bleibt erhalten.
- Shift allein darf in Textfeldern, contenteditable, Dialogen und Terminal keine Dateiauswahl
  verändern. Menü-, Toolbar- und Tastaturausführung nutzen dieselbe Aktion genau einmal.
- „..“ und ein leeres Panel bleiben nicht markierbar; kein Öffnen durch Shift allein.

### Dateien und Ordner gleich behandeln

- Einheitliche Auswahlregeln für Shift allein, Insert, Klick mit Modifier, Bereichsauswahl,
  Musterauswahl, Alles markieren, Auswahl umkehren und Auswahl aufheben.
- Allgemeine Musterauswahl bezieht Ordner genauso wie Dateien ein; die bisherige
  Ordner-Opt-in-Regel aus 059 entfällt. Labels „Alle Einträge markieren“ und
  „Keine Einträge markieren“ verwenden; Hilfetext erklärt Dateien und Ordner.
- Fachlich dateispezifische Aktionen (gleiche Dateierweiterung, Inhaltsvergleich)
  bleiben dateispezifisch, ohne Ordner aus der allgemeinen Auswahl auszuschließen.
- Auswahl bleibt pro Tab; Filter/Refresh/Navigation bereinigen ungültige Ziele.
  Keine versteckten Markierungen, die später unbemerkt kopiert/gelöscht werden.
- Gemischte Datei-/Ordnerauswahl muss in Kopieren, Verschieben, Löschen,
  Packen, Auswahl merken/wiederherstellen und Export konsistent ankommen.

### Auswahlfarben und Zustand

- Gemeinsamer Standard für beide Themes: **blauer Hintergrund #1E40AF und gelbe
  Schrift #FFEB3B**. In zentraler Farbdefinition ablegen, nicht pro Komponente.
- Auswahl hat Vorrang vor Git-, Dateityp-, benutzerdefinierten und Cursorfarben.
  Dateiname und alle textuellen Zellen des markierten Eintrags sind gelb;
  untergeordnete Elemente dürfen diese Farbe nicht überschreiben.
- Beim Abwählen sofort zur zutreffenden Git-/Dateifarbe und normalen Hintergrundfarbe
  zurückkehren. Kein verzögertes Update bis zum nächsten Refresh.
- Cursor/Fokus ist von Markierung getrennt: unmarkierter Cursor bekommt eine Kontur,
  aber keine gelbe Auswahlschrift. Auch im inaktiven Panel bleibt eine echte
  Markierung blau/gelb; Cursorwechsel darf sie nicht verdecken.
- Aktive Auswahl zusätzlich semantisch (`aria-selected`) und über einen sichtbaren
  nichtfarblichen Indikator ausweisen. Kontrast der endgültigen Farbkombination prüfen.
- Vorhandene Farbkonfiguration berücksichtigen: neue Standards für nicht individuell
  überschriebene Werte; bewusste Nutzerfarben nicht still überschreiben. Falls feste
  alte Standardwerte persistiert sind, gezielt migrieren. „Standard wiederherstellen“,
  Vorschau und Legende müssen die neuen Farben verwenden.

### Git-Schriftfarben

| Git-Zustand | Bedeutung | Neue Standardfarbe |
| --- | --- | --- |
| Neu | Im Index hinzugefügt (`A`), einschließlich danach weiterbearbeiteter neuer Datei, sofern kein Konflikt/Löschen vorliegt | Grün; dark #4ADE80 / light #166534 |
| Geändert | Bestehende Datei geändert, ob staged oder unstaged | Blau; dark #60A5FA / light #1D4ED8 |
| Untracked | Noch nicht durch Git verfolgt (`??`) | Rot; dark #F87171 / light #B91C1C |

- „Neu“ und „untracked“ bewusst unterscheiden. Rust-Klassifizierung so ergänzen,
  dass staged Änderung einer bestehenden Datei nicht grün als neu erscheint.
  Vorhandene Statusmodelle prüfen; ts-rs-Bindings bei Änderung regenerieren.
- Konflikt, gelöscht, umbenannt und ignoriert behalten ihre eigenständige Semantik.
  Prioritäten ausdrücklich testen, einschließlich mehrerer Zustände eines Eintrags
  und aggregierter Git-Zustände an Ordnern. Keine pauschale Umfärbung aller staged Werte.
- Bei Markierung werden diese Schriftfarben durch Gelb ersetzt; nach Abwahl sind sie
  wieder sichtbar. Git-Status bleibt über vorhandene Statusinformation/Legende erkennbar.

## Globaler Prüfumfang

Details, kompakte Liste, Miniaturansicht und Baum mit Dateiliste; links/rechts,
aktives/inaktives Panel, mehrere Tabs, beide Themes und benutzerdefinierte Farben.
Die navigierende Baumkomponente von der auswählbaren Dateiliste unterscheiden:
keine neue unabhängige Auswahl im Navigationsbaum einführen.

Alle Auswahlwege aus 059 prüfen, einschließlich Snapshot und Auswahl-Dateien;
Farbvorschauen, Eigenschaften-Farblegende, Kontextmenü, Aktionsleiste,
Menüeinträge aus 040/058 und deren Hilfe/Übersetzungen abgleichen. Lokale und
Remote-Verzeichnisse, Archive und deaktiviertes Git einbeziehen. Andere Dialog-
Checkboxen (etwa Synchronisationsauswahl) nicht mit der Panel-Auswahl vermischen.

## Akzeptanzkriterien und Tests

- [ ] Shift allein markiert und entmarkiert Dateien und Ordner ohne Modifier, ohne Cursorbewegung.
- [ ] Allgemeine Sammel-/Muster-/Bereichsauswahl behandelt Dateien und Ordner identisch;
      nur „..“ ausgeschlossen. Gemischte Operationsziele sind korrekt.
- [ ] Markierte Einträge sind mit Standardfarben in beiden Themes blau/gelb,
      einschließlich aller Textzellen; Cursor ohne Markierung ist unterscheidbar.
- [ ] Markierungswechsel ist sofort sichtbar, auch bei Git-Refresh und Panelwechsel.
- [ ] Git-Fälle `A `, `AM`, ` M`, `M `, `MM`, `??`, Konflikt, Löschen und Rename
      sind durch Rust-Tests auf Bedeutung und Priorität geprüft.
- [ ] UI-Tests für Markierung → Git-Refresh → Abwahl bestätigen korrekte Farben;
      Detail-/Gridansichten sowie inaktives Panel und Ordner sind abgedeckt.
- [ ] Shift-Kombinationen, Autorepeat, beide Shift-Tasten, Fokus-/Tabwechsel und
      Fenster-Blur erzeugen keine zusätzliche oder verspätete Umschaltung.
- [ ] Shift allein in Dialog/Text/Terminal löst keine Panelaktion aus; Quick Look bleibt nutzbar.
- [ ] Bestehende eigene Farben, Defaults/Migration, Legende, Hilfe und de/en stimmen.
- [ ] Manuelle globale Prüfmatrix mit macOS/Windows/Linux, lokal/remote/Archiv,
      Git an/aus und Tastatur ohne Insert dokumentiert.

## Definition of Done

- Ursache der uneinheitlichen Ordnerauswahl und behobene Stellen dokumentieren.
- Relevante Tests, `cargo check` in `src-tauri`, `npm run typecheck`, `npm run build`;
  bei IPC-Änderung `npm run gen:types` erfolgreich.
- `Concept.md` und Menüinventar aktualisieren; neue Semantik gegenüber 059 festhalten.
- Erst nach Umsetzung nach `tickets/done/060-unified-selection-colors-and-toggle.done.md`
  verschieben. Lokaler Commit pro abgeschlossenem Ticket, ohne Push.
