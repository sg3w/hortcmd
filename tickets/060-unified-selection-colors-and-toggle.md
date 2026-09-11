# TICKET-060: Markierung vereinheitlichen, Space-Markierung und Enter-Schnellansicht und klare Auswahl-/Git-Farben

## Priorität und Reihenfolge

**P1 – vorgezogen, als nächstes offenes Ticket bearbeiten.** Expliziter Nutzerwunsch:
vor den weiteren Menüausbauten umsetzen. Keine Abhängigkeit vom Abschluss der
Tickets 040/058; vorhandene Auswahl, Ansichten und Tastatur direkt korrigieren.
Bereits laufende Arbeit sauber abschließen, danach dieses Ticket vorziehen.

## Ziel

Dateien und Ordner lassen sich gleich markieren und wieder abwählen. Ein einzelner
Space-Tastendruck schaltet den Eintrag unter dem Cursor um. Shift behält sein
bisheriges Verhalten; die Schnellansicht wandert von Space auf Enter.
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

### Space zum Markieren, Enter für die Schnellansicht

- **Space = Markierung umschalten** für den Cursor-Eintrag, bei Dateien und
  Ordnern: unmarkiert → markiert, markiert → unmarkiert. Ohne Modifier und ohne
  Cursorbewegung. Ein Tastendruck löst genau eine Umschaltung aus; Autorepeat ignorieren.
- **Shift bleibt wie gehabt**: Bereichsauswahl mit Shift+Pfeil/Shift+Klick und
  bestehende Shift-Kombinationen erhalten. Shift allein schaltet keine Markierung.
  Die zwischenzeitlich vorgesehene Shift-Keyup-Geste entfällt vollständig.
- **Enter = Schnellansicht der Cursor-Datei**, auf macOS über den bisherigen
  Quick-Look-Ablauf, auf Windows/Linux über die vorhandene interne Vorschau.
  Space öffnet auf keiner Plattform mehr eine Vorschau.
- Enter auf Ordnern und „..“ navigiert weiterhin; der bisherige Einstieg in
  unterstützte Archive bleibt erhalten. Dateivorschau ändert keine Auswahl.
  Enter auf normalen Dateien startet nicht zusätzlich ein externes Programm.
- Vorschau nur bei unterstütztem Kontext ausführen: vorhandene Provider-/Archiv-
  Grenzen beachten, bei fehlender Unterstützung verständlich melden, ohne als
  Ersatz zu markieren oder extern zu öffnen. Vorhandenes explizites Öffnen über
  Kontextmenü/Doppelklick und F3-Anzeigen bleibt erreichbar.
- Bestehende Insert-/Mehrfachauswahlkürzel und Cmd/Ctrl-Klick bleiben kompatible
  Alternativen. F9 erhält durch dieses Ticket keine neue Belegung.
- Space/Enter nur bei Dateipanel-Fokus abfangen; Textfelder, contenteditable,
  Dialoge, Menüs, Schaltflächen und Terminal behalten ihre eigene Bedienung.
  Insbesondere Enter zum Bestätigen eines Dialogs darf keine Schnellansicht öffnen.
- Menü, Aktionsleiste, Hilfe und Übersetzungen auf „Space: Markieren/Abwählen“
  und „Enter: Schnellansicht / Ordner öffnen“ abstimmen. Gemeinsame Aktionen nutzen.
- „..“ und leere Panels bleiben nicht markierbar; Space öffnet keine Ordner.

### Dateien und Ordner gleich behandeln

- Einheitliche Auswahlregeln für Space, Insert, Klick mit Modifier, Bereichsauswahl,
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

- [ ] Space markiert und entmarkiert Dateien und Ordner ohne Modifier, ohne Cursorbewegung.
- [ ] Allgemeine Sammel-/Muster-/Bereichsauswahl behandelt Dateien und Ordner identisch;
      nur „..“ ausgeschlossen. Gemischte Operationsziele sind korrekt.
- [ ] Markierte Einträge sind mit Standardfarben in beiden Themes blau/gelb,
      einschließlich aller Textzellen; Cursor ohne Markierung ist unterscheidbar.
- [ ] Markierungswechsel ist sofort sichtbar, auch bei Git-Refresh und Panelwechsel.
- [ ] Git-Fälle `A `, `AM`, ` M`, `M `, `MM`, `??`, Konflikt, Löschen und Rename
      sind durch Rust-Tests auf Bedeutung und Priorität geprüft.
- [ ] UI-Tests für Markierung → Git-Refresh → Abwahl bestätigen korrekte Farben;
      Detail-/Gridansichten sowie inaktives Panel und Ordner sind abgedeckt.
- [ ] Shift allein verändert keine Auswahl; bisherige Shift-Kombinationen funktionieren.
      Space-Autorepeat erzeugt keine wiederholte Umschaltung.
- [ ] Enter zeigt Dateien in der Schnellansicht; Ordner/„..“ und Archive navigieren
      weiterhin korrekt. Space öffnet nie die Vorschau; Enter startet nicht zugleich
      ein externes Programm. Auswahl bleibt bei Vorschau erhalten.
- [ ] Space/Enter in Dialog/Text/Menü/Terminal lösen keine Panelaktion aus;
      Bestätigen und Schließen der Vorschau erzeugt kein durchgereichtes Enter.
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
