# TICKET-059: Menü „Markieren“ mit vollständigen Auswahlfunktionen

## Ziel und Umfang

Das Menü **Markieren** nach dem bereitgestellten Total-Commander-Screenshot vom
11.09.2026, 08:15:00 aufbauen. **Fehlende Funktionen werden in diesem Ticket
implementiert**, nicht nur als Menüeinträge vorbereitet. Vorhandene Funktionen
wiederverwenden und erweitern. Der Screenshot dient als visuelle und funktionale
Vorlage; die folgenden Regeln definieren das Verhalten in hortcmd ausdrücklich.

Baut auf der Menüarchitektur aus [040](040-native-application-menu.md) auf.
[058](058-populate-commander-menu-items.md) übernimmt den allgemeinen Menüabgleich;
dieses Ticket besitzt die neuen Markieren-Funktionen. Die dortige Beschränkung auf
bereits implementierte Funktionen gilt nicht für den ausdrücklich beauftragten
Umfang dieses Tickets. Keine zweite Menüleiste oder parallele Befehlsregistrierung.

## Bestand vor Umsetzung erneut prüfen

- `src/features/commander/selection.ts`: Muster markieren/abwählen und Umkehrung.
- `src/store/panesStore.ts`: Auswahl pro Tab, Filter, Cursor und Ansichtsaufbereitung.
- `src/features/commander/useKeyboard.ts`: bestehende + / - / *-Bedienung.
- `src/components/ui/FileContextMenu.tsx`: Dateinamen/Pfade in die Zwischenablage.
- `src/features/commander/exportList.ts`: Exportfelder und Formatierung für Details.
- `src/store/compareStore.ts` und `src-tauri/src/commands/fs/compare.rs`:
  vorhandener Verzeichnisvergleich; dessen nutzbare Logik prüfen und wiederverwenden.

Neu bzw. zu vervollständigen: explizite Alles-/Keine-Dateiauswahl, gleiche
Erweiterung, Auswahl merken/wiederherstellen, Auswahl speichern/laden,
Detailvarianten der Zwischenablage und panelbasierter Vergleich mit Markierung
und Ausblenden gleicher Dateien. Bereits aus 040/058 vorhandene Anbindungen erhalten.

## Menüeinträge in dieser Reihenfolge

| Gruppe | Eintrag | Verhalten |
| --- | --- | --- |
| 1 | Gruppe markieren… | Dateien nach Muster zusätzlich markieren. |
| 1 | Gruppenauswahl rückgängig… | Dateien nach Muster abwählen; kein Undo des letzten Vorgangs. |
| 1 | Alle Dateien markieren | Alle sichtbaren Dateien markieren. |
| 1 | Keine Dateien markieren | Gesamte Auswahl des aktiven Tabs leeren, einschließlich zuvor markierter Ordner. |
| 1 | Auswahl umkehren | Markierung der sichtbaren Dateien umkehren. |
| 1 | Alle Dateien mit gleicher Erweiterung markieren | Erweiterung der Cursor-Datei als Auswahlkriterium verwenden. |
| 2 | Auswahl merken | Expliziten Auswahlsnapshot im aktiven Tab speichern. |
| 2 | Auswahl wiederherstellen | Letzten passenden Snapshot wiederherstellen. |
| 2 | Auswahl in Datei speichern… | Auswahl in eine lokale Datei exportieren. |
| 2 | Auswahl aus Datei laden… | Gespeicherte Auswahl prüfen und im aktuellen Tab anwenden. |
| 3 | Markierte Namen in Zwischenablage kopieren | Ein Name pro Eintrag. |
| 3 | Namen mit Pfad in Zwischenablage kopieren | Ein vollständiger Pfad bzw. eine Provider-Adresse pro Eintrag. |
| 3 | Namen mit allen Details kopieren | Name, Erweiterung, Größe, Änderungsdatum, Typ und Rechte. |
| 3 | Namen mit Pfad und Details kopieren | Vollständiger Pfad statt Name, sonst dieselben Detailfelder. |
| 4 | Verzeichnisse vergleichen | Dateien beider sichtbarer Panel-Verzeichnisse vergleichen und Unterschiede markieren. |
| 4 | Neue markieren, gleiche verstecken | Fehlende/neuere Dateien markieren und gleiche Dateien in beiden Panels ausblenden. |

Gruppen durch Trenner absetzen. Vorhandener allgemeiner Dateilistenexport darf
als zusätzlicher Eintrag in Gruppe 3 bestehen bleiben. Bei aktivem Vergleichsfilter
zusätzlich **Vergleichsfilter aufheben** anbieten; auch direkt am Filterindikator.

## Auswahlregeln

- Dateiaktionen beziehen sich auf das aktive Panel und seinen aktiven Tab. „..“
  ist nie auswählbar. Ordner bleiben bei den ausdrücklich als „Dateien“ benannten
  Befehlen unverändert; Musterdialoge bieten „Ordner einbeziehen“ als explizite Option.
  Manuelle Ordnerauswahl bleibt möglich. Auswahl umkehren verwendet ebenfalls
  die dokumentierte Datei-Semantik; Menü und zugehöriger Shortcut sind identisch.
- Muster unterstützen die vorhandenen Wildcards `*` und `?`; keine neue Regex-
  Sprache erfinden. Muster markieren addiert, Muster abwählen entfernt Treffer.
- Auswahlbefehle arbeiten auf der aktuell sichtbaren Liste. Beim Ausblenden durch
  Filter dürfen keine unsichtbaren Einträge als spätere Operationsziele markiert
  bleiben; Auswahl bereinigen, Anzahl sichtbar aktualisieren, Snapshot separat erhalten.
- Gleiche Erweiterung: Dateien mit derselben letzten Erweiterung markieren,
  Groß-/Kleinschreibung dabei ignorieren. `a.tar.gz` zählt als `gz`; `.env` ohne
  weiteren Punkt und `README` als ohne Erweiterung. Cursor-Ordner/„..“ deaktiviert
  den Befehl. Vorhandene Markierungen bleiben bestehen.
- Auswahländerungen verschieben den Cursor nicht unnötig. Beide Panels und Tabs
  bleiben unabhängig, außer bei den beiden ausdrücklich beidseitigen Vergleichsbefehlen.

## Auswahl merken und wiederherstellen

- Ein expliziter Snapshot pro Tab, gebunden an die kanonische Verzeichnisadresse
  einschließlich Provider-/Profilidentität und gegebenenfalls Archiv-Innenpfad.
  Keine Bindung an veränderliche Zeilenindizes und keine Speicherung von Credentials.
- „Merken“ ersetzt den vorherigen Snapshot; auch eine leere Auswahl ist gültig.
  Navigieren in ein anderes Verzeichnis verwirft ihn. Keine dauerhafte Speicherung
  über App-Neustarts; dafür dient der Datei-Export.
- „Wiederherstellen“ ersetzt die aktuelle Auswahl durch noch existierende, sichtbare
  Snapshot-Einträge. Fehlende/ausgefilterte Einträge mit Anzahl melden; nicht automatisch
  navigieren oder Filter ändern. Ohne passenden Snapshot ist der Befehl deaktiviert.
- Refresh/Sortierung verändern die Snapshot-Namen nicht; nach Umbenennen darf kein
  zufällig an derselben Zeilenposition liegender Eintrag ausgewählt werden.

## Auswahl in Datei speichern und laden

- Versioniertes UTF-8-JSON-Format (`version`, `location`, `entries` mit Name/Typ)
  statt unescaped zeilenbasierter Namen, damit Unicode, Tabs und Zeilenumbrüche
  verlustfrei bleiben. Keine Kompatibilität zu einem unbekannten TC-Dateiformat behaupten.
- Auswahl speichern verwendet ausschließlich markierte Einträge, keinen impliziten
  Cursor-Fallback; auch leere Auswahl darf als gültige Datei gespeichert werden.
  Lokalen Dateidialog und vorhandene Überschreibungsbestätigung wiederverwenden.
- Lesen/Schreiben und Formatvalidierung im Rust-Backend; bestehende IPC-/VFS-
  Adressmodelle nutzen. Grenzen: maximal 10 MiB und 100.000 Einträge, bei Überschreitung
  klare Fehlermeldung ohne teilweise Auswahländerung. Keine Zugangsdaten exportieren.
- Laden validiert Version, Struktur, Ort und Eintragsnamen. Nur direkte Kindnamen,
  keine absoluten Pfade, Traversal oder eingebettete Provider-Adressen als Einträge.
  Namensregeln des Quellproviders beachten; keine lokale Pfadverkettung für Remote-URIs.
- Bei gleichem Ort Vorschau mit Treffern/fehlenden Einträgen und Bestätigung zum
  Ersetzen der Auswahl. Bei anderem Ort explizit anbieten, Namen auf das aktuelle
  Verzeichnis anzuwenden, oder abzubrechen; niemals automatisch verbinden/navigieren.
- Duplikate zusammenfassen; nur existierende, sichtbare Einträge passenden Typs
  übernehmen. Fehler oder Abbrechen erhalten die bisherige Auswahl vollständig.

## Zwischenablage mit Details

- Alle vier Varianten verwenden dieselbe Zielmenge: markierte sichtbare Einträge,
  sonst gültiger Cursor-Eintrag, ohne „..“. Ohne Ziel deaktivieren.
- Reihenfolge entspricht der aktuellen Sortierung. Namen-/Pfadvarianten behalten
  das bestehende zeilenweise Format; Detailvarianten sind TSV mit Kopfzeile und
  korrekt gequoteten Tabs, Zeilenumbrüchen und Anführungszeichen.
- Vorhandene Exportformatierung wiederverwenden. Größe/Datum nach aktuellen
  Einstellungen; fehlende Remote-Metadaten leer bzw. als nicht verfügbar kennzeichnen,
  nicht als null Byte oder erfundene Rechte. Keine rekursive Größenberechnung auslösen.
- Remote-Pfade korrekt als Provider-Adressen, ohne Passwörter oder Tokens ausgeben.
  Schreibfehler der Zwischenablage sichtbar melden.

## Verzeichnisse vergleichen / Neue markieren, gleiche verstecken

- Beide aktiven Panel-Tabs, jeweils nur die direkte Verzeichnisebene vergleichen.
  Keine Synchronisation, kein Kopieren/Löschen und kein automatischer rekursiver Scan.
  Bestehender umfassender Synchronisationsdialog unter Befehle bleibt bestehen.
- Rust-Vergleichslogik wiederverwenden/ergänzen; UI übernimmt die Ergebnisdarstellung.
  Fähigkeiten aus 031 prüfen: bei nicht unterstützten Providern/Archiven begründet
  deaktivieren. Lokal und Remote unterstützen, soweit deren Fähigkeiten dies erlauben.
- Vergleich vor Anwendung bestehender Anzeigefilter ermitteln; Filter beeinflussen
  nur die sichtbare Auswahl. Namen exakt vergleichen, keine mehrdeutigen
  Groß-/Kleinschreibungs-Zuordnungen zwischen unterschiedlichen Providern.
- Gleich: gleicher Dateiname, Größe und Änderungszeit innerhalb der dokumentierten
  Provider-Zeitauflösung. Das ist Metadatengleichheit, kein garantierter Inhaltsvergleich.
  Fehlende Metadaten, Symlinks und Typkonflikte als nicht sicher vergleichbar behandeln;
  sie bleiben sichtbar und werden nicht als gleich ausgeblendet.
- „Verzeichnisse vergleichen“ ersetzt die Dateiauswahl in beiden Panels durch
  sichtbare Dateien, die auf der Gegenseite fehlen oder abweichen (jeweils beide
  vorhandenen Seiten einer Abweichung). Ordner werden nicht automatisch markiert.
- „Neue markieren, gleiche verstecken“ markiert je Seite fehlende Dateien sowie
  Dateien mit neuerem Änderungsdatum. Andere Unterschiede bleiben sichtbar;
  insbesondere gleiche Zeit bei anderer Größe wird als Unterschied angezeigt,
  nicht willkürlich als „neuer“ markiert. Gleiche Dateien werden beidseitig verborgen.
- Vergleichsfilter ist temporär, mit sichtbarem Status und Aufheben-Aktion; vorhandene
  Schnell-/Systemfilter bleiben erhalten. „..“ und Ordner bleiben erreichbar.
- Vor einem neuen Vergleich bestehende Vergleichsfilter aufheben. Navigation,
  Tabwechsel, Disconnect oder Inhaltsänderungen/Refresh invalidieren das Ergebnis
  und heben den Vergleichsfilter auf. Veraltete asynchrone Ergebnisse dürfen keine
  neue Tab-Auswahl verändern. Vergleich muss abbrechbar bleiben.

## Tastatur und Integration

- Gemeinsame Befehlsdefinition aus 040 nutzen. Vorhandene + / - / * bleiben;
  zusätzlich Num+, Num-, Num* sowie Cmd/Ctrl+Num+ (alle), Cmd/Ctrl+Num- (keine),
  Alt+Num+ (gleiche Erweiterung), Num/ (wiederherstellen), Umschalt+F2 (Vergleich)
  nach Konfliktprüfung implementieren; Modifier vor allgemeinen F-Tasten behandeln.
- Alle Aktionen ohne Nummernblock per Menü vollständig erreichbar. Plattformgerechte
  Kürzel anzeigen; native Textbearbeitung, Dialoge und SSH-Terminal haben Vorrang.
- Labels, leere Zustände, Ergebnisse und Fehlermeldungen de/en; Tastaturnavigation,
  Fokuswiederherstellung, Checkboxen und deaktivierte Gründe nach 040/043.

## Akzeptanzkriterien und Tests

- [ ] Alle 16 Screenshot-Funktionen sind fachlich implementiert und als Menüaktionen
      ausführbar; vorhandene Logik wird wiederverwendet, fehlende ergänzt.
- [ ] Gruppen, Trenner, Kürzel und Zustände entsprechen der beschriebenen Struktur.
- [ ] Tests für Muster, Ordneroption, „..“, Filter, leere Auswahl, Erweiterungen,
      Unicode und voneinander unabhängige Tabs bestehen.
- [ ] Snapshot bleibt bei Sortierung/Refresh korrekt; Navigation, gelöschte und
      umbenannte Einträge sowie leere Snapshots sind geprüft.
- [ ] Auswahl-Dateien bestehen Roundtrip-Tests einschließlich Sonderzeichen;
      falsche Version, Traversal, Größenlimit, Duplikate, anderer Ort, Abbrechen
      und Schreibfehler verändern keine Auswahl unbeabsichtigt.
- [ ] Vier Zwischenablagevarianten sind mit Mehrfachauswahl, Cursor-Fallback,
      TSV-Sonderzeichen und fehlenden Remote-Metadaten geprüft.
- [ ] Vergleich geprüft für nur links/rechts, gleich, neuer/älter, gleiche Zeit bei
      anderer Größe, unbekannte Zeiten, Typkonflikte, Symlinks und leere Verzeichnisse.
- [ ] Vergleichsfilter lässt sich verlustfrei aufheben; Navigation/Refresh/Disconnect
      und verspätete Ergebnisse erzeugen keine unsichtbaren Operationsziele.
- [ ] Menü-/Shortcutaktionen laufen genau einmal; keine Durchleitung bei Text-,
      Dialog- oder Terminalfokus. macOS, Windows, Linux und Tastatur ohne Numpad geprüft.

## Definition of Done

- Implementierung, relevante Rust-/Frontendtests und manuelle Plattformprüfungen
  abgeschlossen; `cargo check` in `src-tauri`, `npm run typecheck`, `npm run build`
  bestehen. Bei IPC-Typänderungen zusätzlich `npm run gen:types`.
- `Concept.md` und das Menüinventar aus 058 um tatsächliche Funktionen, Format und
  bewusst von TC abweichende Semantik ergänzen.
- Erst nach Umsetzung nach `tickets/done/059-mark-menu-and-selection-functions.done.md`
  verschieben; Ticketanlage bedeutet keine fertige Implementierung.
- Lokalen Commit nach Ticketabschluss erstellen, ohne Push.
