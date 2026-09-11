# TICKET-064: Verzeichnisvergleich als gegenüberliegende doppelte Liste

## Ziel

Den bestehenden Dialog „Verzeichnisse vergleichen / synchronisieren“ als
**doppelte Liste** darstellen: links die Quelldatei, rechts ihr Gegenstück,
mit gemeinsamem Vergleichsstatus in der Mitte. Zusammengehörige Dateien müssen
immer auf derselben Zeile liegen. Vorlage ist der Nutzer-Screenshot
„Bildschirmfoto 2026-09-11 um 16.43.54.png“ von Total Commander.

Der Screenshot dient als Layoutvorlage. Dessen zusätzliche Optionen wie
„Asymmetrisch“, Inhaltsvergleich und Datum ignorieren gehören nicht automatisch
zum Auftrag. Vorhandene Vergleichs- und Kopierfunktionen erhalten; Fokus dieses
Tickets ist die präzise beidseitige Ergebnisdarstellung und Bedienung.

## Bestand

- `src/components/ui/CompareDialog.tsx`: existierender Vergleichsdialog mit Pfaden,
  Rekursionsoption, Statusfiltern, virtualisierten Ergebnissen, Auswahl und
  Kopieren links → rechts / rechts → links.
- `src/store/compareStore.ts`: Öffnen des Dialogs aus beiden Panel-Verzeichnissen.
- `compareDirs` / `syncCopy` in `src/ipc/client.ts` und Rust-Vergleichslogik:
  gemeinsame Ergebniszeile mit relativem Pfad, Existenz je Seite, Größe, Datum
  und Status. Aktuellen Stand und Provider-Unterstützung vor Umsetzung prüfen.
- Kein neuer, paralleler Vergleichsdialog. Der panelbasierte Markierungsvergleich
  aus 059 bleibt eine andere Darstellung; dieser Auftrag betrifft den Dialog.

## Aufbau des Dialogs

### Kopfbereich

- Linke und rechte Verzeichnisadresse in zwei klar beschrifteten Feldern/Bereichen
  über der jeweiligen Dateiseite anzeigen. Lange Adressen kürzen, vollständigen
  Wert zugänglich per Tooltip und kopierbar anbieten.
- Bestehende Aktion „Vergleichen“ und Option „Unterverzeichnisse einbeziehen“
  beibehalten. Während des Laufs Fortschritt/Teilresultate sichtbar machen.
- Statusfilter: **Unterschiede**, **Nur links**, **Nur rechts**, **Gleiche**, jeweils
  mit Anzahl. Filter ändern nur die sichtbaren Ergebnisse, nicht Vergleichslogik.
- Keine neuen Vergleichsmodi oder neuen Dateisystem-Picker allein für die Vorlage.

### Gemeinsame Tabelle mit zwei Dateiseiten

Spaltenaufbau (links nach rechts):

```text
Auswahl | Name / relativer Pfad | Größe | Datum | Status | Datum | Größe | Name / relativer Pfad
        |              LINKS                 |        |                 RECHTS             |
```

- Zwei optisch gleichwertige Spaltengruppen „Links“ und „Rechts“, jeweils unter
  ihrer Verzeichnisadresse; schmale, fest erkennbare Statusspalte in der Mitte.
- **Eine Ergebniszeile pro relativem Pfad**, kein unabhängiges Aneinanderreihen
  zweier separat sortierter Dateilisten. Die Zeile repräsentiert das Dateipaar.
- Name auf beiden Seiten anzeigen. Bei rekursivem Vergleich den relativen Pfad
  einschließlich Unterverzeichnis anzeigen, damit gleiche Basenamen unterscheidbar sind.
- Jede Seite hat ihren eigenen Größen- und Datumswert. Größen rechtsbündig;
  Zeit-/Größenformat nach vorhandenen Einstellungen. Header bleiben beim Scrollen stehen.
- Fehlende Datei: auf dieser Seite Name/Größe/Datum als „—“ bzw. klarer Leerzustand;
  niemals Werte der anderen Seite wiederholen oder eine fehlende Datei als 0 Byte zeigen.
- Lange Namen einzeilig kürzen, vollständigen relativen Pfad per Tooltip/zugänglichem
  Text erhalten. Gleiche Zeilenhöhe links/rechts; keine versetzten Umbrüche.
- Vertikales Scrollen und Virtualisierung gemeinsam für die ganze Tabelle.
  Ein horizontaler Scrollbereich darf bei schmalen Fenstern genutzt werden;
  Spaltengruppen dürfen nicht unabhängig vertikal auseinanderlaufen.
- Sortierung, falls angeboten, sortiert ganze Dateipaare gemeinsam. Standard nach
  relativem Pfad; keine unabhängige Links-/Rechts-Sortierung. Stabile Zeilenidentität
  über relativen Pfad und Vergleichslauf, niemals über sichtbaren Zeilenindex.

### Statusanzeige in der Mitte

| Vergleichsergebnis | Darstellung | Bedeutung |
| --- | --- | --- |
| Gleich | `=` + „Gleich“ als Tooltip/zugänglicher Name | Gleich gemäß bestehender Vergleichsmethode. |
| Nur links | Symbol + „Nur links“ | Rechts existiert kein Gegenstück. |
| Nur rechts | Symbol + „Nur rechts“ | Links existiert kein Gegenstück. |
| Links neuer | Symbol + „Links neuer“ | Zeitvergleich ergibt neuere linke Datei. |
| Rechts neuer | Symbol + „Rechts neuer“ | Zeitvergleich ergibt neuere rechte Datei. |
| Unterschiedlich | `≠` + „Unterschiedlich“ | Abweichung ohne eindeutige neuere Seite. |

- Zustände als Text/Symbol zugänglich machen, nicht ausschließlich als Farbe.
- Vergleichsstatus ist **keine bereits ausgewählte Kopieraktion**. Ein Statussymbol
  darf nicht versehentlich Synchronisation starten oder eine Richtung umschalten.
- Die bestehende Metadatenvergleichsmethode im Dialog kurz benennen:
  „Vergleich nach Größe und Änderungsdatum“. `=` behauptet keine nachgewiesene
  Bytegleichheit. Falls der tatsächliche Code andere Regeln hat, korrekt beschriften.
- Unbekannte/fehlende Metadaten, Fehler oder abgeschnittene Ergebnisse sichtbar
  ausweisen; keinesfalls als „Gleich“ darstellen.

## Auswahl und Kopierrichtungen

- Eine Checkbox pro Dateipaar; Auswahl hebt die vollständige Zeile auf beiden
  Seiten hervor. Tastaturnavigation bewegt sich ebenfalls durch Dateipaare.
- Vorhandene Mehrfachauswahl und „alle sichtbaren auswählen“ erhalten. Bei
  Filterwechsel unsichtbare Auswahl aus der ausführbaren Auswahl entfernen,
  damit verborgene Zeilen nicht unbemerkt kopiert werden.
- Zwei eindeutig beschriftete Aktionen beibehalten:
  **„Auswahl links → rechts kopieren“** und **„Auswahl rechts → links kopieren“**.
  Pro Richtung Anzahl der tatsächlich kopierbaren ausgewählten Dateien anzeigen.
- Nur Zeilen mit existierender Quelle und unterstütztem Quell-/Zielkontext für
  die jeweilige Richtung verwenden. Ohne gültige Auswahl den Button deaktivieren.
  Ausgewählte Einträge ohne Quelle in der Richtung transparent als nicht kopierbar melden.
- Ein neuer Vergleich leert die bisherige Auswahl. Änderungen von Pfaden oder
  Optionen invalidieren alte Ergebnisse; veraltete Stream-Batches dürfen keine
  neuen Resultate oder Zeilenauswahl überschreiben.
- Bestehende Sicherheits-/Überschreibungs- und Fehlerbehandlung erhalten.
  Unmittelbar vor Kopieren Quell-/Zielkontext erneut prüfen. Kein automatisches
  Spiegeln, Löschen oder „neuere gewinnt“ nur durch Öffnen des Dialogs.
- Nach Kopieren erneut vergleichen und Status beider Seiten aktualisieren.
  Pfadidentische Quelle/Ziel darf keine Selbstkopie auslösen.

## Fußbereich und Bedienung

- Zusammenfassung getrennt ausweisen: Gesamtzahl der Vergleichspaare, sichtbar,
  ausgewählt; zusätzlich Gleiche, Unterschiede, Nur links, Nur rechts.
  Bei Streaming als vorläufig kenntlich machen, bei Limit/Fehler Warnung erhalten.
- Dialog skalierbar; helle/dunkle Themes und de/en unterstützen.
- Fokus, Checkboxen, Sortierheader und Scrollbereich per Tastatur bedienbar;
  Screenreader muss linke und rechte Metadaten samt Status zuordnen können.
- Kein zusätzlicher Schalter „einzelne/doppelte“ nötig: doppelte Liste wird die
  Standardansicht dieses Dialogs. Die vorhandenen Filter decken gleiche/einseitige
  Dateien ab; Begriffe aus dem Screenshot nicht als unklare Ansichtsmodi übernehmen.

## Beispiel zur Abnahme

```text
Links                         Status         Rechts
bericht.txt  2 KB  10:00       Gleich         10:00  2 KB  bericht.txt
nur-links.txt 1 KB 09:00       Nur links      —      —     —
—             —   —           Nur rechts     11:00  3 KB  nur-rechts.txt
src/app.ts    4 KB 12:00       Links neuer    08:00  3 KB  src/app.ts
```

Das Beispiel illustriert Paarbildung und fehlende Gegenstücke; reale Werte werden
mit Datum und dem eingestellten Format angezeigt. Auch bei Filtern/Scrollen bleibt
jede Zuordnung horizontal erhalten.

## Akzeptanzkriterien und Tests

- [ ] Links/rechts besitzen jeweils Name/Pfad, Größe und Datum; Status liegt mittig.
- [ ] Gleiche und unterschiedliche Dateien sowie nur links/rechts vorhandene
      Dateien werden korrekt gepaart; fehlende Werte erscheinen eindeutig.
- [ ] Rekursive Pfade mit gleichen Basenamen, Unicode und langen Namen sind eindeutig.
- [ ] Filter, Auswahl, eventuelle Sortierung und Virtualisierung erhalten die Paarung.
- [ ] Große gestreamte Ergebnislisten bleiben flüssig; Header/Zeilen laufen nicht
      auseinander, auch nach Resize und Wiederöffnen des Dialogs.
- [ ] Kopierrichtungen arbeiten nur auf sichtbarer gültiger Auswahl, ohne Selbstkopie
      und ohne implizite Lösch-/Spiegeloperation; danach korrekte neue Ergebnisse.
- [ ] Kein Ergebnis, laufender/fehlgeschlagener/limitierter Vergleich, fehlende
      Metadaten und Remote-Disconnect sind verständlich dargestellt.
- [ ] Komponenten-/Integrationstests prüfen Paarbildung, Status, Filter/Selektion
      und Richtungsauswahl; bestehende Vergleichslogik behält ihre Tests.
- [ ] Manuell schmale/breite Fenster, 200%-Zoom, de/en, hell/dunkel, Tastatur sowie
      macOS/Windows/Linux geprüft. Keine Screenshot-Pixelkopie zulasten der Bedienbarkeit.

## Definition of Done

- Bestehenden Dialog umgestellt, keine doppelte Vergleichsimplementierung.
- Relevante Tests, `cargo check` in `src-tauri`, `npm run typecheck`,
  `npm run build`; bei IPC-Änderungen zusätzlich `npm run gen:types` bestehen.
- `Concept.md` um die doppelte Ergebnisliste und deren Bedienung ergänzen.
- Erst nach Umsetzung nach `tickets/done/064-directory-compare-double-list.done.md`
  verschieben. Lokaler Commit nach Ticketabschluss, ohne Push.
