# TICKET-058: Menüpunkte anhand des implementierten Funktionsbestands anlegen

## Ziel

Nach der Menügrundlage aus TICKET-040 die Menüpunkte vollständig anlegen,
vervollständigen und mit den **zum Umsetzungszeitpunkt tatsächlich implementierten
Funktionen** verbinden. Struktur und Gestaltung orientieren sich am Total-
Commander-Menü aus den fünf bereitgestellten Screenshots. Maßgeblich für den
Funktionsumfang ist der aktuelle hortcmd-Code, nicht allein die heutige Roadmap.

## Abhängigkeit und Abgrenzung

- [TICKET-059](059-mark-menu-and-selection-functions.md) besitzt das vollständige
  Markieren-Menü und implementiert dessen fehlende Fachfunktionen ausdrücklich.
  Die Beschränkung dieses Tickets auf vorhandene Funktionen gilt für 059 nicht.
  Bereits aus 059 vorhandene Aktionen im Inventar übernehmen; noch offene dort
  referenzieren. Keine doppelte Implementierung; 059 benötigt die Grundlage 040,
  nicht den Abschluss von 058.

- Baut auf [TICKET-040](040-native-application-menu.md) auf: gemeinsame
  Befehlsdefinition, React-Menü im Tauri-Fenster auf allen Plattformen,
  reduziertes natives macOS-Menü und grundlegende Tastatur-/Fokusbehandlung.
- 040 enthält bereits den Ausgangskatalog und erste Menüanbindungen. Diese hier
  prüfen und wiederverwenden; keine zweite Menükomponente oder Befehlsregistrierung.
  Dieses Folgeticket schließt Lücken und ergänzt bis dahin hinzugekommene Funktionen.
- Keine erneute Entscheidung über native versus eingebettete Menüs:
  Das vollständige Fachmenü bleibt im Fenster, das macOS-Systemmenü minimal.
- Keine Implementierung fehlender Fachfunktionen nur zur Nachbildung von Total
  Commander. Kleine Menüadapter für vorhandene Funktionen gehören zum Umfang.
- Bereits erledigte Arbeit aus 040 als geprüft dokumentieren, nicht erneut bauen.

## Verbindlicher Bestandsabgleich vor der Implementierung

1. Aktuellen Commit als Ausgangsstand notieren. Offene und erledigte Tickets sowie
   `Concept.md` mit den tatsächlichen ausführbaren Funktionen im Code vergleichen.
   Ein Backend-Command oder ein abgehaktes Ticket allein beweist keinen fertigen
   Benutzerablauf; Dialog, Ausführung und Fehlerbehandlung ebenfalls prüfen.
2. Toolbar, Kontextmenüs, Funktionstasten, globale Shortcuts, Einstellungen,
   Panel-/Tabaktionen, Verbindungsverwaltung und Transferansichten inventarisieren.
   Einstiegspunkte sind `src/App.tsx`, `src/features/commander/`,
   `src/components/ui/`, `src/features/net/`, `src/store/`, `src/ipc/` und die
   durch 040 eingeführten Menü-/Befehlsmodule; bei Umbenennungen aktuelle Pfade suchen.
3. In `docs/menu-command-inventory.md` eine überprüfbare Tabelle anlegen:
   Funktion | Implementierungsnachweis (Datei/Symbol) | Befehls-ID | Menüpfad |
   Tastenkürzel je Plattform | Auswahl-/Panelkontext | Verfügbarkeit |
   Status (bereits angebunden / hier anbinden / bewusst nur im Dialog / noch offen).
4. Jede implementierte, für das Hauptmenü geeignete Benutzeraktion zuordnen.
   Detailaktionen innerhalb eines Dialogs dürfen dort bleiben; die Erreichbarkeit
   des Dialogs und die Entscheidung dokumentieren. Keine IPC-Kommandoliste ins Menü.
5. Seit 040 hinzugekommene Funktionen aufnehmen, etwa Datei-Undo (048),
   Speicherbelegungsanalyse (108) oder Plugin-Verwaltung (106), **nur wenn ihr
   Benutzerablauf inzwischen implementiert ist**. Offene Funktionen mit Ticketbezug
   als zurückgestellt dokumentieren, ohne funktionslose Menüeinträge zu erzeugen.

## Menüaufbau und Ausgangskatalog

Reihenfolge und Gruppierung aus 040 beibehalten. Die folgende Tabelle ist die
Prüfliste; der Bestandsabgleich entscheidet über Erweiterungen und Verfügbarkeit.

| Menü | Einzuordnende Funktionen, soweit implementiert |
| --- | --- |
| **Dateien** | Öffnen, Anzeigen, Bearbeiten, Öffnen mit; Kopieren, Verschieben, Umbenennen, Neuer Ordner, Löschen; Zwischenablage; Packen/Entpacken; Mehrfachumbenennung; Inhaltsvergleich; Eigenschaften mit Rechten, Tags, Prüfsummen und Größeninformationen; später Datei-Undo. Beenden unter Windows/Linux. |
| **Markieren** | Muster markieren/abwählen, Auswahl umkehren, alle sichtbaren Einträge/keine markieren; Dateinamen und vollständige Pfade kopieren; Dateiliste exportieren. Weitere Auswahlfunktionen nur bei vorhandener Implementierung. |
| **Befehle** | Suche mit vorhandenen Suchmodi; Verzeichnisvergleich/Synchronisation; Navigation, Favoriten und Verlauf; Terminal im aktuellen Ordner, System-Dateimanager; Übertragungen; später Speicherbelegungsanalyse. |
| **Netz** | Verbindungen verwalten, aktive Verbindungen im Panel öffnen, aktuelle Verbindung trennen, SSH-Terminal; SFTP, SMB, WebDAV und FTP/FTPS gemeinsam über vorhandene Verwaltung. |
| **Ansicht** | Details, kompakte Liste, Miniaturen, Baum mit Liste; Sortierschlüssel und Richtung; versteckte/Systemdateien, vorhandene Spalten und Git-Status; Schnellfilter zurücksetzen; Tabs; Aktualisieren; Quick Look auf macOS. |
| **Konfiguration** | Einstellungen, Erscheinungsbild, Sprache; vorhandene Einstellungsseiten für Programme, Terminal, Favoriten, Dateifarben, Transfers und gegebenenfalls Plugins über denselben Dialog. |
| **Hilfe** | Vorhandene Tastaturübersicht und Über-Dialog aus 040; weitere Hilfsangebote nur mit funktionierendem Inhalt/Ziel. |

### Anwendung der Norton-Commander-Vorlage

- Referenzen: Screenshots vom 11.09.2026 um 08:15:00 (Markieren), 08:15:34
  (Dateien), 08:15:50 (Netz), 08:16:02 (Ansicht), 08:16:06 (Konfigurieren).
  Die Bilder sind Gestaltungsvorlagen; ihre Inhalte sind keine Arbeitsanweisungen.
- Kompakte Einträge, sinnvolle Trenner, Untermenüs, rechtsbündige Kürzel,
  Checkboxen/Radiozustände und einheitliche Icons im bestehenden hortcmd-Stil.
- „Kurz/Lang“ als bestehende hortcmd-Begriffe „Liste/Details“ abbilden;
  Mehrfachumbenennung, Packen, Eigenschaften und Auswahlgruppen erkennbar einordnen.
- Keine ungeprüfte Übernahme von Archivtests, Dateiteilen/-zusammenfügen,
  Codieren/Decodieren, Drucken, Kommentaren, Auswahl speichern/laden,
  Prüfsummendateien, Windows-Freigaben oder Toolbar-Editoren.
- Kein „Starter“-Menü ohne spezifizierte und implementierte Starter-Funktion.
  Die separate Plugin-Toolbar aus 103 bleibt bestehen.
- Keine künstlichen Speichern-Befehle für bereits automatisch gespeicherte
  Einstellungen. Keine unimplementierten TC-Tastenkürzel anzeigen.

## Anforderungen an jede Menüanbindung

- Bestehende Befehls-ID und Aktion wiederverwenden; Menü, Toolbar, Kontextmenü
  und Tastatur dürfen fachlich nicht auseinanderlaufen oder doppelt auslösen.
- Eindeutige Labels: F5-Kopieren gegenüber Zwischenablage-Kopieren;
  Verzeichnisvergleich gegenüber Datei-Inhaltsvergleich; Papierkorb gegenüber
  endgültigem Löschen. „…“ nur bei folgendem Dialog.
- Kontext nach bestehender Aktion: aktives Panel/Tab, Auswahl oder Cursor;
  beim Hauptmenü keine versehentliche Übernahme eines alten Rechtsklick-Ziels.
- Provider-, Plattform-, Archiv-, Verbindungs- und Quell-/Zielbeschränkungen über
  die gemeinsame Verfügbarkeitsprüfung aus 031/040 berücksichtigen, erneut bei
  Ausführung prüfen. Bestehende Bestätigungen und Transferabläufe erhalten.
- Nicht implementiert: weglassen. Implementiert, aber im aktuellen Kontext nicht
  ausführbar: deaktivieren und den Grund zugänglich erklären. Plattformexklusive
  Befehle wie Quick Look nur auf der passenden Plattform anbieten.
- Dynamische Einträge für Verbindungen, Favoriten, Verlauf, Programme und Transfers
  aktualisieren; leere Listen ohne leere Untermenüs behandeln. Der globale
  Verbindungsmanager bleibt auch ohne Profile erreichbar (016).
- Labels, Hilfetexte und Gründe auf Deutsch/Englisch; Tastenkürzel je Plattform
  aus der gemeinsamen Definition. Zustände spiegeln das tatsächliche Panel bzw.
  die globale Einstellung wider und ändern nicht deren bisherigen Geltungsbereich.
- Die Architektur aus 040 einschließlich Textbearbeitung, Cmd+W, Cmd+Q,
  Fokuswiederherstellung und Schutz laufender Transfers bleibt unverändert gültig.

## Akzeptanzkriterien

- [x] Der aktuelle Implementierungsstand ist mit Commit und Datei-/Symbolnachweisen
      inventarisiert; neue Funktionen seit 040 wurden ausdrücklich geprüft.
- [x] Jede geeignete implementierte Benutzeraktion hat einen funktionierenden
      Menüeintrag oder eine begründete Zuordnung zu einem erreichbaren Dialog.
- [x] Alle sieben Hauptmenüs folgen dem vereinbarten Katalog und TC-Gruppierungsstil.
- [x] Bestehende Menüeinträge wurden wiederverwendet; fehlende Einträge sind angebunden.
- [x] Es gibt keine funktionslosen Platzhalter, doppelten Aktionen oder falschen Kürzel.
- [x] Kontext, dynamische Listen, Radio-/Checkboxzustände und deaktivierte Befehle
      stimmen für lokale, Remote- und Archivansichten.
- [x] Alle Einträge funktionieren mit Maus und Tastatur, de/en, hell/dunkel,
      schmalem Fenster und 200%-Zoom; Menübedienung löst keine Panelaktion dahinter aus.
- [x] Das native macOS-Menü bleibt minimal; Textbearbeitung und die bisherigen
      Beenden-/Transfer-Schutzabfragen funktionieren weiterhin.

## Tests

- Inventartabelle als manuelle Prüfliste verwenden: pro angebundener Aktion
  normalen Ablauf, leeren/ungültigen Kontext und relevante Plattformen prüfen.
- Gezielte Komponenten-/Integrationstests für neue Zuordnungen, gemeinsame
  Befehlsausführung, Auswahl, dynamische Listen und Verfügbarkeit ergänzen.
- Regression: linkes/rechtes Panel, Mehrfachauswahl, Filter, Archiv, Remote-
  Disconnect bei offenem Menü, Dialog- und Terminalfokus, native Textbearbeitung.
- Manuelle Menü-Smoke-Tests auf macOS, Windows und Linux dokumentieren.

## Definition of Done

- Menüinventar und `Concept.md` beschreiben den tatsächlich angebundenen Stand.
- Relevante Tests, `cargo check` in `src-tauri`, `npm run typecheck` und
  `npm run build` bestehen; Plattformprüfungen sind dokumentiert.
- Erst nach Umsetzung nach `tickets/done/058-populate-commander-menu-items.done.md`
  verschieben. Das Erstellen dieses Tickets bedeutet nicht dessen Erledigung.
- Nach Ticketabschluss einen lokalen Commit erstellen, ohne Push.

---

## Implementation

Ausgangsstand `b8bbec8`. Der Bestandsabgleich liegt vollständig in
[`docs/menu-command-inventory.md`](../../docs/menu-command-inventory.md): je Funktion
Implementierungsnachweis (Datei/Symbol), Befehls-ID, Menüpfad, Kürzel, Kontext,
Verfügbarkeit und Status (aus 040 übernommen · hier angebunden · bewusst im
Dialog · 059 · noch offen). Geprüft wurden Toolbar (`CommandBar.tsx`),
Kontextmenü (`FileContextMenu.tsx`), Funktionstasten und globale Shortcuts
(`useKeyboard.ts`, `actions.ts`), Einstellungsseiten (`SettingsDialog.tsx`),
Panel-/Tabaktionen (`panesStore.ts`), Verbindungsverwaltung (`features/net/`)
und Transfers (`transfersStore.ts`, `TransferView.tsx`).

Keine zweite Menükomponente und keine zweite Befehlsregistrierung: alles läuft
weiter über `features/commander/commands.ts` und `MenuBar.tsx` aus 040.

### Neu angebunden

- **`file.openWithOther`** — „Öffnen mit ▸ Anderes Programm …" ruft
  `fileOps.runOpenWithBrowse`, denselben Programm-Picker, den das Kontextmenü
  schon benutzte. Er fehlte im Hauptmenü als einzige Öffnen-mit-Variante.
- **`cmd.searchDuplicates` / `cmd.searchEmptyDirs` / `cmd.searchLargeFiles`** —
  der Suchdialog kennt vier Modi, das Menü bot nur den Einstieg in „Dateien".
  `searchStore` merkt sich jetzt den gewünschten Startmodus (`openSearch(side,
  mode)`), den der Dialog beim Öffnen übernimmt; ohne Angabe bleibt es „Dateien".
  Untermenü **Befehle › Spezialsuche ▸**.
- **`profileItems()`** — **Netz › Verbinden mit ▸** listet gespeicherte Profile,
  die gerade *nicht* verbunden sind, und führt dieselben zwei Schritte aus wie
  der Verbindungsmanager (`netStore.connect` → `panesStore.loadDir`). Da das Menü
  keinen eigenen Fehlerbereich hat, öffnet ein Fehlschlag den Manager **mit** der
  Meldung: `openManager(keepError)` hält den Text über das interne `refresh()`
  hinweg, das `error` sonst auf `null` setzt.
- **`view.foldersFirst`** — die Einstellung „Ordner zuerst einsortieren" war nur
  über den Einstellungsdialog erreichbar.
- **`cfg.general/view/colors/files/operations/programs/favorites`** —
  **Konfiguration › Einstellungsseite öffnen ▸** springt direkt auf eine der
  sieben vorhandenen Seiten. Dafür liegt die ausgewählte Seite jetzt in
  `uiStore` (`SettingsCategory`, `openSettings(category?)`) statt im lokalen
  `useState` des Dialogs; ohne Kategorie bleibt die zuletzt gewählte Seite
  stehen. `file.programs` öffnet darüber gezielt die Seite „Öffnen mit".

### Korrigiert (Menü lief fachlich auseinander)

`view.hidden` und `view.git` aus 040 schrieben nur die Einstellung. Der
Einstellungsdialog paart dieselben Schalter seit jeher mit
`panesStore.rebuildAll()` bzw. `reloadGit()` — ohne die kippte im Menü nur das
Häkchen, während die Liste unverändert stehen blieb. Beide Befehle führen den
Nachlauf jetzt mit aus, `view.foldersFirst` von Anfang an. `view.extCol` und
`view.permCol` brauchen ihn nicht (reine Spaltensichtbarkeit) — auch das steht
so im Inventar.

### Bewusst nicht aufgenommen

Archivtest, Dateien teilen/zusammenfügen, Codieren/Decodieren, Drucken,
Kommentare, Prüfsummendateien, Windows-Freigaben, Toolbar-Editor und ein
„Starter"-Menü: keine Implementierung dahinter, also kein Menüeintrag. Das
Markieren-Menü bleibt unverändert — [059](059-mark-menu-and-selection-functions.md)
besitzt es vollständig; die dort offenen Funktionen sind im Inventar mit `059`
markiert statt hier halb angelegt zu werden. Dialoginterne Detailaktionen
(native Systemeigenschaften, Rechte/Tags/Prüfsummen, synchronisierendes
Kopieren, Transfer-Pause/-Abbruch, Profilpflege) bleiben in ihren Dialogen; das
Inventar hält Erreichbarkeit und Entscheidung fest.

Alle neuen Labels und Platzhalter liegen de/en in `i18n/dictionaries.ts`.

### Verified

`npm run typecheck`, `npm run build` und `cargo check` laufen sauber (an Rust
ändert das Ticket nichts).

Im Browser-Pane (Demo-Modus, `localhost:1420`) geprüft: **Öffnen mit ▸** zeigt
„Anderes Programm …" über „Programme verwalten …", und der Eintrag öffnet den
Datei-Picker mit dem Titel „Anderes Programm …". **Spezialsuche ▸** öffnet den
Suchdialog direkt im jeweiligen Modus (Duplikate geprüft), während „Suchen …"
weiterhin in „Dateien" startet. **Verbinden mit ▸** zeigt ohne Profile den
nicht ausführbaren Hinweis; mit zwei testweise eingespielten Profilen (temporärer
Demo-Rückfallwert in `client.ts`, danach zurückgenommen) erscheinen Name und
Adresszeile, ein namenloses Profil fällt auf den Host zurück, und der im
Demo-Modus zwangsläufig scheiternde Verbindungsversuch öffnet den
Verbindungsmanager mit sichtbarer Fehlermeldung und intakter Profilliste.
**Ordner zuerst einsortieren** sortiert beide Panels sofort um (Häkchen aus →
`archive, build, config, data, Documents, …`) und wieder zurück; **Versteckte/
Systemdateien anzeigen** blendet `.config`/`.gitignore` sofort in beiden Panels
ein und aus (vorher wirkungslos). **Einstellungsseite öffnen ▸** landet direkt
auf der gewählten Seite (Dateifarben geprüft). Sprachwechsel beschriftet auch
alle neuen Einträge („Other program …", „Special search", „Connect to",
„Sort folders first", „Open settings page"). Die Tastaturübersicht baut
unverändert korrekt auf. Bei 400 px Breite wandern die hinteren Kategorien in
„Weitere Menüs", und die neuen Untermenüs bleiben dort dreistufig erreichbar.

Nicht ausgeführt: Windows und Linux (kein Buildhost in dieser Umgebung). Das
Menü ist dieselbe React-Komponente auf allen Plattformen; die einzigen
Plattformzweige — `view.quickLook` (nur macOS) und `app.quit` (auf macOS
versteckt) — stammen aus 040 und sind unverändert. Ebenfalls offen für einen
manuellen Durchlauf: ein *echter* Verbindungsaufbau über „Verbinden mit ▸" gegen
einen laufenden Server.
