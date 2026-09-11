# TICKET-040: Commander-Menü im Tauri-Fenster und minimales macOS-Systemmenü

## Ziel und beschlossene Ausrichtung

Die vollständige Menüleiste liegt auf macOS, Windows und Linux im Tauri-Fenster,
oberhalb der Toolbar. Die vorhandene React-Leiste wird zu einem bedienbaren
Anwendungsmenü ausgebaut. Das native macOS-Menü wird auf App-, Textbearbeitungs-
und Fensterfunktionen reduziert.

Diese am 11.09.2026 bestätigte Entscheidung ersetzt die bisherige Anforderung
dieses Tickets, alle Fachmenüs unter macOS in die globale Systemleiste zu legen.
Ticketnummer und Dateiname bleiben für bestehende Verweise erhalten.

Technische Abgrenzung: „Menü im Tauri-Fenster“ bedeutet hier React/WebView.
Tauris native Menü-API erzeugt auf macOS ein globales Systemmenü; sie wird für
den reduzierten nativen Teil eingesetzt. Sie erzeugt dort keine eingebettete
Menüleiste. Siehe [Tauri: Window Menu](https://v2.tauri.app/learn/window-menu/).

## Bestand und Vorlage

- `src/App.tsx`: Dateien, Markieren, Befehle, Netz, Ansicht, Konfiguration und
  Hilfe sind bereits beschriftet; nur Konfiguration öffnet die Einstellungen.
- `src/features/commander/actions.ts`, `fileOps.ts`, `useKeyboard.ts` und
  `selection.ts`: vorhandene Dateiaktionen, Tastaturmodell und Musterauswahl.
- `src/features/commander/CommandBar.tsx`: Navigation, Suche, Vergleiche,
  Mehrfachumbenennung, Listenexport und Ansichtswechsel.
- `src/components/ui/FileContextMenu.tsx`: Zwischenablage, Eigenschaften,
  Öffnen mit, Terminal, Dateinamen/Pfade kopieren und macOS Quick Look.
- `src/store/panesStore.ts` und `settingsStore.ts`: Tabs, Sortierung,
  Filter, Ansichten, Sprache, Darstellung und weitere Einstellungen.
- `src/features/net/netStore.ts`: vorhandener Verbindungsmanager;
  SFTP/SSH, SMB, WebDAV und FTP/FTPS sind implementiert.

Vorlage sind die fünf vom Nutzer bereitgestellten Total-Commander-Screenshots
vom 11.09.2026 (08:15:00, 08:15:34, 08:15:50, 08:16:02, 08:16:06).
Übernommen werden Kategorien, kompakte Dropdowns, Trenner, Untermenüs,
Zustandsmarkierungen und rechts ausgerichtete Tastenkürzel. Die Bilder zeigen
nicht das geöffnete Menü „Befehle“; dessen Inhalt wird aus hortcmd abgeleitet.
Nicht vorhandene TC-Funktionen werden nicht allein wegen der Vorlage ergänzt.

## Menüstruktur für die erste Umsetzung

Reihenfolge: **Dateien · Markieren · Befehle · Netz · Ansicht · Konfiguration · Hilfe**.
Hilfe darf rechts stehen, bleibt aber Teil derselben Tastaturnavigation.
Die folgenden Gruppen werden durch Trenner getrennt. „…“ bedeutet, dass ein
Dialog folgt. Vorhanden bezeichnet die Funktion, nicht eine fertige Menüanbindung.

### Dateien

1. Öffnen; Anzeigen… (F3); Bearbeiten (F4 gemäß Editor-Einstellung);
   Öffnen mit ▸ konfigurierte Programme und Programme verwalten.
2. Kopieren… (F5); Verschieben… (F6); Umbenennen… (F2);
   Neuer Ordner… (F7); Löschen… (F8); Endgültig löschen… (Umschalt+F8).
3. In Zwischenablage kopieren; Ausschneiden; Einfügen (Cmd/Ctrl+C/X/V).
   F5-Kopieren und Zwischenablage-Kopieren müssen eindeutig verschieden heißen.
4. Packen… (Alt+F5, vorhandenes ZIP-Packen); Entpacken…;
   Mehrfach umbenennen…; Dateien nach Inhalt vergleichen… (linke/rechte Cursor-Datei).
5. Eigenschaften… (Alt+Enter), einschließlich vorhandener Rechte, Finder-Tags,
   Prüfsummen und Ordner-/Volume-Größe innerhalb dieses Dialogs.
6. Beenden auf Windows/Linux; auf macOS im nativen App-Menü.

Vorhandene Bestätigungen, Papierkorb-Einstellung und Transferdialoge beibehalten.
Keine separaten Prüfsummendatei- oder Archivtest-Befehle vortäuschen:
Prüfsummenanzeige und Kopierverifikation sind bereits vorhanden, Erzeugen und
Verifizieren von Prüfsummendateien jedoch nicht durch dieses Ticket abgedeckt.

### Markieren

Erweiterung: [TICKET-059](059-mark-menu-and-selection-functions.md) implementiert
das vollständige Markieren-Menü einschließlich bisher fehlender Funktionen.
Die folgenden Einschränkungen gelten nur für den Erstumfang von 040;
059 definiert die erweiterte Datei-/Ordner-Auswahlsemantik verbindlich.

1. Gruppe markieren… (+); Gruppe abwählen… (-); Auswahl umkehren (*).
2. Alle sichtbaren Einträge markieren; Keine Einträge markieren.
   Dafür bestehende Musterauswahl verwenden; „..“ bleibt ausgeschlossen.
   Ordner sind eingeschlossen, daher bewusst „Einträge“ statt „Dateien“.
3. Dateinamen kopieren; Vollständige Pfade kopieren; Dateiliste exportieren…
   (vorhandener Exportdialog mit seiner Auswahl des Umfangs und der Felder).

Auswahl gilt für die gefilterte Liste des aktiven Tabs. Auswahl merken,
wiederherstellen, aus Datei laden und nach gleicher Erweiterung auswählen sind
kein Erstumfang. Es gibt keine dauerhaft funktionslosen Platzhalter dafür.

### Befehle

1. Suchen… (Alt+F7): vorhandener Dialog mit Datei-/Inhaltssuche, Duplikaten,
   leeren Ordnern und großen Dateien; keine vier neuen Dialoge.
2. Verzeichnisse vergleichen / synchronisieren… (bestehender Vergleichsdialog).
3. Navigation ▸ Zurück (Alt+Links), Vorwärts (Alt+Rechts), Übergeordneter Ordner;
   Favoriten ▸ vorhandene Favoriten; Zuletzt besuchte Ordner ▸ Verlauf und Löschen.
4. Terminal im aktuellen Ordner öffnen (lokal oder SSH bei SFTP);
   Im System-Dateimanager öffnen (nur unterstützte lokale Ziele).
5. Übertragungen ▸ laufende/ wartende Vorgänge als dynamische Einträge, die
   den jeweiligen vorhandenen Transferdialog öffnen; Pause/Fortsetzen/Abbrechen
   bleiben dort. Leerer Zustand: „Keine Übertragungen“ ohne ausführbare Aktion.

Das Hauptmenü verwendet den aktuellen Ordner. Die abweichende Zielbestimmung
beim Rechtsklick auf einen Ordner bleibt auf das Kontextmenü beschränkt.

### Netz

1. Verbindungen verwalten… → `useNet.openManager()`, immer erreichbar,
   auch ohne gespeicherte Profile oder aktive Verbindung.
2. Aktive Verbindungen ▸ Verbindung im aktiven Panel öffnen.
3. Aktuelle Verbindung trennen… → vorhandener Disconnect-Ablauf, nur bei
   zuordenbarer aktiver Remote-Verbindung; Auswirkungen auf Transfers beachten.
4. SSH-Terminal öffnen → vorhandenes Terminal, nur für aktive SFTP-Verbindung.

Neue Profile werden im vorhandenen Manager angelegt. Es gibt keine Windows-
spezifischen Freigabe-, Laufwerksbuchstaben- oder PORT-Transfer-Platzhalter.
Die Protokolle werden gemeinsam verwaltet und nicht auf ein FTP-Menü reduziert.

### Ansicht

1. Darstellung ▸ Details, Liste (kompakt), Miniaturansicht, Baum mit Liste;
   Radioauswahl für das aktive Panel.
2. Sortieren nach ▸ Name, Erweiterung, Größe, Datum;
   Aufsteigend / Absteigend als eindeutige Zustände.
3. Versteckte/Systemdateien anzeigen; Erweiterungsspalte; Berechtigungsspalte;
   Git-Status anzeigen (Checkboxen auf den bestehenden Einstellungen).
4. Schnellfilter zurücksetzen (nur mit aktivem Filter).
5. Neuer Tab (Cmd/Ctrl+T); Aktuellen Tab schließen (Cmd/Ctrl+W);
   Ordner unter Cursor in neuem Tab öffnen.
6. Aktuelles Panel aktualisieren.
7. Quick Look auf macOS, mit bestehender lokaler Verfügbarkeit.

Ansicht/Sortierung/Tabs gelten wie bisher pro Panel bzw. Tab; die genannten
Einstellungen für Spalten, versteckte Dateien und Git bleiben global.
Kein neues Persistenzmodell durch das Menü. Ein expliziter Sortierrichtungsadapter
ist bei Bedarf zu ergänzen, da `setSort` bisher beim gleichen Schlüssel umschaltet.
TC-Schnellansicht im Gegenpanel, vertikale Panelanordnung, benutzerdefinierte
Spaltensätze und „nur markierte Dateien“ sind nicht Teil der ersten Umsetzung.

### Konfiguration

1. Einstellungen… → vorhandener Einstellungsdialog.
2. Erscheinungsbild ▸ System, Hell, Dunkel.
3. Sprache ▸ Deutsch, Englisch.

Programme/Dateizuordnungen, Terminal, Favoriten, Dateifarben und Transferoptionen
bleiben im vorhandenen Einstellungsdialog erreichbar. Neue Direkteinstiege dürfen
nur denselben Dialog auf der passenden Seite öffnen. Kein „Einstellungen speichern“:
Die vorhandene Speicherung erfolgt automatisch. Kein Toolbar-Editor in diesem Ticket.

### Hilfe

- Tastaturbelegung…: kleine neue Übersicht aus derselben Befehlsdefinition.
- Über hortcmd…: kleiner neuer Dialog mit tatsächlicher Version und MIT-Lizenzhinweis;
  wiederverwendet vom nativen App-Menü.

Kein „Starter“-Menü ohne implementierte Starter-Funktion. Plugin-Befehle bleiben
in der separaten Plugin-Toolbar aus TICKET-103; keine vorweggenommene Menü-API.

## Minimales natives macOS-Menü

- **hortcmd**: Über hortcmd, Einstellungen… (Cmd+,), Dienste, hortcmd ausblenden
  (Cmd+H), Andere ausblenden, Alle einblenden, Beenden (Cmd+Q).
- **Bearbeiten**: notwendige native Textbearbeitung (Rückgängig/Wiederholen,
  Ausschneiden/Kopieren/Einfügen, Alles auswählen) für WebView-Eingabefelder.
  Native Text-Rückgängig-Funktion ist kein Undo für Dateioperationen (TICKET-048).
- **Fenster**: Minimieren und nötige native Fensterverwaltung.
  Cmd+W bleibt im Dateipanel „Tab schließen“; kein konkurrierender nativer
  Fenster-Schließen-Accelerator darf dieses Kommando abfangen.
- Keine vollständige Kopie von Dateien/Markieren/Befehle/Netz/Ansicht im Systemmenü.
- Über und Einstellungen dürfen bewusst doppelt erreichbar sein, verwenden aber
  dieselben Dialoge. Beenden muss denselben Schutz für laufende Übertragungen
  verwenden wie der bisherige Fenster-Schließen-Ablauf; kein ungeprüfter Prozess-Exit.

## Umsetzung und Verhalten

- Eigene React-Menükomponente statt klickbarer `span`-Platzhalter in `App.tsx`;
  zugängliche Menubar-Primitives passend zur bestehenden Radix-UI verwenden.
- Gemeinsame Befehlsdefinition mit stabiler ID, Übersetzungsschlüssel,
  Tastenkürzel, Ausführung, Sichtbarkeit, Verfügbarkeit und Checkbox-/Radiozustand.
  Menü, Toolbar, Kontextmenü und Tastatur nutzen gemeinsame Aktionen statt
  duplizierter Dateioperationen. Vorhandenes `runAction` und Store-Aktionen erweitern.
- UI-Aktionen bleiben im Frontend, Datei-/Systemarbeit hinter vorhandenen Tauri-
  Commands im Rust-Backend. Reduziertes natives Menü in einem dedizierten Rust-
  Modul, eingebunden in `src-tauri/src/lib.rs`; UI-Ereignisse eindeutig weiterleiten.
- Kontext: aktives Panel, aktiver Tab, Auswahl oder Cursor gemäß vorhandener Aktion.
  Beim Menüöffnen den Dateikontext bewahren; vor Ausführung Existenz, Verbindung,
  Quell-/Zielrechte und Fähigkeiten erneut prüfen (TICKET-031).
- Nicht verfügbare implementierte Befehle deaktivieren und den Grund zugänglich
  erklären. Dauerhaft nicht implementierte Funktionen nicht anbieten.
- Pfeiltasten navigieren zwischen Menüs/Einträgen, Enter öffnet/führt aus,
  Escape schließt und stellt Fokus wieder her. Menübedienung darf weder den
  Schnellfilter füllen noch Dateiaktionen hinter dem Menü auslösen.
- Dialoge und Terminals haben Vorrang vor globalen Datei-Shortcuts. Eingaben,
  `contenteditable` und native Bearbeitungsbefehle korrekt behandeln. Ein Tastendruck
  darf über native Events und WebView-Listener zusammen nur eine Aktion auslösen.
- Bestehende F-Tasten und Alt-Kürzel erhalten; Cmd auf macOS, Ctrl auf Windows/Linux
  für die vorhandenen Plattformkürzel. Keine TC-Kürzel anzeigen, die noch nicht
  implementiert sind. Neue Kürzel erst nach Konfliktprüfung gemeinsam registrieren.
- Labels und Beschreibungen vollständig de/en. Aktuelle Zustände bei Sprach-,
  Theme-, Tab-, Auswahl- und Verbindungswechsel aktualisieren.
- Kompakte Gestaltung im vorhandenen Theme; Einträge mit Label, optionalem Icon,
  Statusmarkierung und Kürzel. Lange Menüs scrollen innerhalb des Fensters;
  bei schmalen Fenstern überlaufende Kategorien über einen zugänglichen
  „Weitere Menüs“-Eintrag erreichbar halten. Keine abgeschnittenen Befehle.

## Abgleich mit bestehenden Tickets

| Ticket | Bezug und Zuständigkeit |
| --- | --- |
| 016 | Toolbar-Einstieg zum Verbindungsmanager und Bereinigung des Laufwerkswählers; Netz-Menü verbindlich hier in 040. |
| 031 | Gemeinsame Provider-Fähigkeiten und Sperren auch für das Hauptmenü. |
| 043 | Übergreifende Accessibility/Übersetzung; 040 liefert bereits vollständige Menübedienbarkeit. |
| 048 | Später Datei-Undo unter Dateien mit konkretem Aktionsnamen; derzeit kein Datei-Undo-Platzhalter. |
| 056 | Später Speicherbelegungsanalyse unter Befehle; vorhandene Ordnergröße bleibt unter Eigenschaften. |
| 103 / 106 | Separate Plugin-Toolbar und Plugin-Verwaltung; kein Starter-Menü in 040. |
| 012 (done) | Bestehende React-Dialoge wiederverwenden, keine neuen nativen Dialogfenster. |

Die Menügrundlage einschließlich Ausgangskatalog bleibt in 040. Das ausdrücklich
beauftragte [Folgeticket 058](058-populate-commander-menu-items.md) prüft danach den
zu diesem Zeitpunkt implementierten Funktionsbestand und ergänzt fehlende
Menüpunkte. Bereits hier angebundene Aktionen werden dort wiederverwendet;
es entsteht keine zweite Menüarchitektur. 040 ist nicht von 058 abhängig.

## Akzeptanzkriterien

- [ ] Alle sieben Kategorien sind auf allen drei Plattformen im Fenster bedienbar.
- [ ] Alle oben für den Erstumfang genannten Befehle sind korrekt angebunden;
      Hilfeübersicht und Über-Dialog sind vorhanden, ohne weitere Fachfunktionen zu erfinden.
- [ ] macOS zeigt nur den beschriebenen reduzierten nativen Menüumfang.
- [ ] Netz → Verbindungen verwalten funktioniert auch ohne Profile/Verbindungen.
- [ ] Aktives Panel, Cursor/Auswahl und Quell-/Zielkontext bleiben korrekt;
      Modal-/Menü-/Terminalfokus verhindert unbeabsichtigte Dateiaktionen.
- [ ] Provider-Sperren gelten einheitlich; Disconnect während geöffnetem Menü
      verhindert die Ausführung gegen eine ungültige Sitzung.
- [ ] Menü, Kontextmenü, Toolbar und Tastatur führen dieselbe Aktion genau einmal aus.
- [ ] Textbearbeitung funktioniert; Cmd+W schließt den Panel-Tab; Cmd+Q und
      alle Beenden-Wege respektieren laufende Übertragungen und Abbrechen.
- [ ] Checkboxen, Radiozustände, de/en und Themewechsel aktualisieren sich korrekt.
- [ ] Tastatur, Screenreader, 200%-Zoom und schmale Fenster bleiben bedienbar.

## Tests und Definition of Done

- Komponenten-/Integrationstests für Routing, Zustände, deaktivierte Aktionen,
  Fokuswiederherstellung, Escape/Enter und doppelte Shortcut-Ausführung.
- Lokale Ordner, leere/gefilterte Liste, Mehrfachauswahl, Archiv und Remote-Ziele;
  Profil ohne Verbindung sowie Disconnect bei offenem Menü prüfen.
- Manuell auf macOS/Windows/Linux: Platzierung, Tastatur, Textfelder, Dialoge,
  Terminal, Menüs bei 200%-Zoom, de/en und hell/dunkel testen.
- macOS zusätzlich: native Copy/Paste/Undo, Cmd+W im Panel und Cmd+Q mit laufender
  Übertragung inklusive Abbrechen; keine Umgehung der bestehenden Schutzabfrage.
- `cargo check` in `src-tauri`, `npm run typecheck`, `npm run build` bestehen.
- `Concept.md` um tatsächliche Menüstruktur und Befehlszuordnung aktualisieren.
- Erst nach Implementierung und Verifikation nach `tickets/done/` verschieben und
  als `040-native-application-menu.done.md` benennen.
- Nach abgeschlossenem Ticket lokalen Commit erstellen, ohne Push.
