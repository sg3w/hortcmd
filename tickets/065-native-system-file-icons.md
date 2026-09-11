# TICKET-065: Native Systemicons für Dateien und Ordner verwenden

## Ziel

Dateien und Ordner mit den Standardicons des jeweiligen Betriebssystems anzeigen,
wie im nativen Dateimanager. Das Nachladen kann langsamer als die bisherigen
statischen Icons sein; Navigation und Bedienung müssen trotzdem unmittelbar
reagieren. **Systemicons werden die Standardeinstellung**, die bisherigen einfachen
Icons bleiben als schneller wählbarer Modus und als Lade-/Fehlerfallback verfügbar.

## Bestand

`src/lib/fileIcon.tsx` wählt derzeit anhand von Erweiterung/Ordnerstatus ein Lucide-
Icon aus. `RowIcon` wird unter anderem in `src/features/panel/FileTable.tsx` für
Dateilisten eingesetzt. Alle weiteren Verwendungen und eigenständigen Dateiicons
vor Umsetzung inventarisieren. Es existiert noch keine native Iconauflösung in
 diesem Modul; tatsächlichen aktuellen Backend-Stand ebenfalls prüfen.

## Anforderungen

### Darstellung und Einstellung

- Einstellung unter Dateiansicht: **Dateisymbole: Systemicons / Einfache Icons**.
  Erläuterung: „Systemicons entsprechen dem Betriebssystem und können beim ersten
  Laden länger brauchen.“ Labels und Erklärung de/en, Auswahl dauerhaft speichern.
- Neue Installationen und bisherige Einstellungen ohne diese Option verwenden
  Systemicons. Explizit gewählte einfache Icons bei späteren Starts erhalten.
- Native Icons für Dateitypen, normale und spezielle Ordner sowie lokale Dateien
  mit individuellem Icon nutzen, soweit das Betriebssystem sie bereitstellt.
- Details, kompakte Liste, Miniaturansicht und Baum-/Dateiauswahldialoge einheitlich
  behandeln, soweit sie Datei-/Ordnericons anzeigen. Reine Aktionsicons der Toolbar,
  Menübefehle und Navigationssymbol „..“ bleiben bestehende UI-Icons.
- Icongrößen und HiDPI-Skalierung beachten, keine unscharfe Vergrößerung kleiner
  Bitmaps. Native farbige Icons nicht mit Git-/Auswahltextfarben einfärben.
- Dateinamen, Git-Farben, Auswahl und Cursorverhalten bleiben unverändert.
  Ein nachgeladenes Icon darf keinen Zeilenhöhenwechsel oder Scrollsprung verursachen.
- Systemicons sind keine neuen Inhalts-Thumbnails oder Quick-Look-Vorschauen.
  Bereits vorhandene echte Vorschauen behalten ihre bisherige Rolle.
- **Symlinks erhalten ein kleines Pfeilsymbol unten links am Datei-/Ordnericon.**
  Die Kennzeichnung gilt für Systemicons, einfache Icons und Lade-/Fehlerfallbacks,
  in allen betroffenen Dateiansichten und Größen. Sie bleibt in beiden Themes
  und bei Auswahl erkennbar, ohne das Haupticon wesentlich zu verdecken.
- Vorhandene Symlink-Metadaten (z. B. `is_symlink`) verwenden; keine zusätzliche
  Zielauflösung oder Remote-Abfrage allein für den Pfeil. Auch defekte Symlinks
  kennzeichnen, soweit ihr Linkstatus bekannt ist. Ein nativer bereits enthaltener
  Linkpfeil darf nicht doppelt überlagert werden; die Plattformadapter müssen dies
  berücksichtigen bzw. ein Basisicon ohne Overlay für die gemeinsame Darstellung liefern.
- Overlay rein visuell auf das Icon setzen, ohne Zeilenhöhe oder Klickfläche zu
  verändern. Den Symlink-Status zusätzlich als zugängliche Beschreibung bereitstellen.

### Native Auflösung im Backend

- Betriebssystemspezifische Implementierung in einem Rust-Service mit dünnen Tauri-
  Commands; Plattformadapter per `cfg` trennen. Aktuelle offizielle APIs bzw. geeignete
  gepflegte Bindings bei Implementierung prüfen: macOS native Workspace-Icons,
  Windows Shell-Icons, Linux Desktop-Icon-Theme/MIME-Zuordnung.
- Thread-/Main-Thread-/COM-Anforderungen der jeweiligen API beachten. Langsame
  Abfragen dürfen weder Dateilisting noch UI-Thread blockieren; keine unkontrollierten
  Tasks pro Datei starten. Falls eine native API den Main Thread benötigt, dort
  nur den erforderlichen Aufruf ausführen und restliche Arbeit auslagern.
- Gebündelte Anfragen mit stabiler Identität und gewünschter Pixelgröße unterstützen;
  pro Eintrag Erfolg oder Fallback liefern. Rückgabe über bestehende IPC-Konventionen,
  bei neuen Datentypen ts-rs nutzen. Bilddaten über einen begrenzten, geeigneten
  Transport bereitstellen; keine beliebigen zusätzlichen Dateizugriffs-Endpunkte.
- Für Remote-Adressen und Archiv-Innenpfade anhand von Dateityp/Name ein lokales
  System-Dateitypicon nutzen. Keine Remote-Dateien herunterladen, Archive entpacken
  oder Verbindungen öffnen, nur um ein Icon zu ermitteln.
- Nicht unterstützte Desktop-Umgebung, unbekannter Typ, verschwundene Datei oder
  fehlerhafte native Abfrage: zuverlässiges generisches Systemicon bzw. bestehendes
  einfaches Icon. Kein wiederholter Fehlerdialog je Zeile.

### Ladeverhalten und Cache

- Verzeichnisinhalt sofort anzeigen; Icons anschließend asynchron ergänzen.
  Währenddessen stabiles einfaches Icon verwenden.
- Nur sichtbare Einträge plus kleinen Overscan priorisieren. Gleiche laufende
  Anfragen zusammenfassen; begrenzte Warteschlange und Parallelität verwenden.
- Beim Navigieren veraltete Ergebnisse nicht an gleichnamige Einträge eines neuen
  Verzeichnisses hängen. Nicht mehr benötigte Requests verwerfen/abbrechen, soweit
  technisch möglich. Generations-/Kontextidentität berücksichtigen.
- Begrenzter LRU-Cache mit dokumentiertem Byte-/Eintragslimit. Standard-Dateitypicons
  nach Typ, Größe, Skalierung und Theme-Kontext wiederverwenden; individuelle lokale
  Datei-/Ordnericons zusätzlich nach Pfad/Identität und relevantem Änderungsstand.
  Nicht sämtliche Icons derselben Erweiterung fälschlich als identisch behandeln.
- Cache bei relevanten Änderungen invalidieren: Theme/Icon-Theme, Skalierung,
  Iconmodus, individuelle Dateiänderung und Dateizuordnung. Falls eine Plattform
  keine Änderungsbenachrichtigung liefert, begrenzte Lebensdauer/Refresh festlegen.
- Kurzes negatives Caching für fehlende Icons; Speicher/Bildressourcen beim Entfernen
  freigeben. Keine unbegrenzt wachsenden Object-URLs oder Bilddaten in persistentem
  Frontend-State. Einfache-Icons-Modus löst keine nativen Abfragen aus.

## Akzeptanzkriterien und Tests

- [ ] Systemicons sind standardmäßig aktiv und entsprechen auf macOS, Windows und
      Linux der verfügbaren nativen Iconauflösung; einfache Icons sind umschaltbar.
- [ ] Unterschiedliche Dateitypen, normale/spezielle Ordner, individuelle Icons,
      unbekannte Endungen und HiDPI sehen korrekt aus, ohne Layoutsprünge.
- [ ] Remote-/Archiv-Einträge erhalten Typicons ohne Download oder Extraktion.
- [ ] Fehlende native Unterstützung und einzelne Abfragefehler führen zu Fallbacks.
- [ ] Datei-, Ordner- und defekte Symlinks zeigen genau einen kleinen Pfeil unten
      links; beide Iconmodi, Fallback/Nachladen, Auswahl, Themes und HiDPI geprüft.
      Normale Dateien/Ordner erhalten keinen Pfeil.
- [ ] Schnelle Navigation, gleichnamige Dateien in verschiedenen Ordnern, Scrollen
      und Umschalten während laufender Requests zeigen keine falschen Icons.
- [ ] Tests für Request-Deduplizierung, Cachegrenzen, Invalidierung und späte Antworten.
- [ ] Manuelle Vergleichsmessung mit einfachen Icons und kaltem/warmem Systemicon-
      Cache in großem Verzeichnis: Zeit bis bedienbare Liste, Icon-Nachladezeit,
      Speicher und UI-Reaktionsfähigkeit dokumentieren. Keine blockierende Vollauflösung
      aller Verzeichniseinträge vor Anzeige; Anzahl paralleler Abfragen ist begrenzt.
- [ ] Alle betroffenen Ansichten, de/en, beide Themes, Auswahl/Git-Farben und
      persistierte Moduswahl geprüft. Plattformtests einschließlich Linux-Fallback.

## Definition of Done

- Plattformadapter, gemeinsame Iconkomponente und Einstellung integriert;
  Einschränkungen und Performance-Messwerte in `Concept.md` dokumentiert.
- Relevante Tests, `cargo check` in `src-tauri`, `npm run typecheck`,
  `npm run build`; bei IPC-Änderungen `npm run gen:types` erfolgreich.
- Erst nach Umsetzung nach `tickets/done/065-native-system-file-icons.done.md`
  verschieben. Lokaler Commit nach Ticketabschluss, ohne Push.
