# hortcmd – Konzept

Ein Dateicommander im Stil von Total Commander, umgesetzt mit **Rust + Tauri v2**
(Backend) und **React + TypeScript** (Frontend).

Dieses Dokument hält Architektur, Entscheidungen und den priorisierten Backlog fest.
Es ist die Referenz für die weitere Umsetzung.

---

## 1. Zielbild

Ein Zwei-Fenster-Dateimanager (Dual-Pane) mit tastaturzentrierter Bedienung nach dem
Vorbild von Total Commander – aber bewusst mit macOS-Stärken (Quick Look, Finder-Tags,
Spotlight) und Entwickler-Features (Git, Terminal, SFTP) als Alleinstellungsmerkmal.

- Zwei gleichwertige Dateifenster, je mit eigenen Tabs und Pfad/Verzeichnis.
- Tastatur zuerst: `Tab` wechselt das Fenster, Pfeiltasten bewegen den Cursor,
  `Insert` markiert (`Space` = Quick Look auf macOS), `Enter` öffnet, `Backspace`
  navigiert hoch.
- Funktionstastenleiste F3–F8 (Ansehen, Bearbeiten, Kopieren, Verschieben, Neuer
  Ordner, Löschen).
- Spaltenansicht (Name · Endung · Größe · Datum), sortierbar per Spaltenklick.
- Nebenläufige, abbrechbare Dateioperationen mit Fortschritt und Kollisionsdialog.

---

## 2. Technologie-Entscheidungen

| Bereich | Wahl | Begründung |
|---|---|---|
| Backend | **Rust / Tauri v2** | Nativer Dateisystemzugriff, kleine Binaries, sichere IPC. |
| Sprache Frontend | **TypeScript** | Typsicherheit über die IPC-Grenze, bessere DX mit TanStack. |
| Tabelle | **TanStack Table** (headless) | Spalten, Sortierung, Resizing; kein vorgegebenes Markup. |
| Virtualisierung | **TanStack Virtual** | Verzeichnisse mit 10k+ Einträgen bleiben flüssig. |
| State | **Zustand** | Schlanker Store, feingranulare Selektoren, transiente Updates. |
| Overlays/Menüs | **Radix UI** (shadcn-Stil) | Kontextmenü, Dialog, Select – zugänglich, im Repo besitzbar. |
| Icons | **Lucide** | Leichtgewichtig, tree-shakeable; Endung→Icon in `lib/fileIcon.tsx`. |
| Styling | **Tailwind CSS** | Utility-first, teilt CSS-Variablen mit dem Theme. |
| Tastatur/Auswahl | **Eigenbau** | Kein Lib-Grid bildet das TC-Verhalten exakt ab. |
| Typen-Sync | **ts-rs** | Generiert TS-Typen aus den Rust-Structs → garantiert synchron. |
| Bundler | **Vite** | Schnell, Tauri-Standard. |

**Rust-Crates:** `tauri`, `serde`, `sysinfo` (Laufwerke), `dirs` (Home),
`ts-rs` (Typen), `base64` (Bild-Vorschau), `opener` (F4 Öffnen), `zip` (ZIP, inkl.
AES-Passwort), `tar` + `flate2`/`xz2` (tar/tar.gz/tar.xz), `sevenz-rust` (7z, inkl.
Passwort).

### Bewusst NICHT gewählt
- **React Aria Grid** – überschneidet sich mit TanStack Table und weicht vom TC-Modell ab.
- **Redux/MobX** – für diesen Umfang Overhead; Zustand genügt.

---

## 3. Architektur

```
┌──────────────────────────── Frontend (WebView) ────────────────────────────┐
│  React + TS                                                                 │
│   store/        Zustand-Stores (panes, settings, clipboard, ops, transfers) │
│   features/     panel (TanStack Table+Virtual) · commander (Tastatur/Ops)   │
│   components/ui Radix-Dialoge (Settings, Kollision, Vorschau, Transfer)      │
│   ipc/client.ts invoke-Wrapper + Event-Bus + Demo-Fallback                  │
└──────────────────────────────────┬──────────────────────────────────────────┘
                invoke(cmd,args)    │    Events: fs-progress · fs-done · fs-collision
┌──────────────────────────────────▼──────────────────── Backend (Rust) ──────┐
│  commands/fs/  dir · file · preview · archive                                │
│  std::fs · sysinfo · dirs · zip · base64 · opener                           │
│  Threads für Transfers; pub(crate) Prog/spawn_op/OpDone geteilt             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Datenfluss (Ordner öffnen):** `Enter` → `useKeyboard`/`navigate.openEntry` →
`store.loadDir/loadArchive` → `ipc.listDir` → Rust `list_dir` → `DirListing` →
Store sortiert/filtert → nur das betroffene Panel re-rendert.

**Datenfluss (Transfer):** F5 → `fileOps` startet Transfer (fire-and-forget) →
Rust-Thread arbeitet, sendet `fs-progress`; bei Konflikt `fs-collision` (blockiert bis
`resolve_collision`); Abschluss `fs-done` → Frontend refresht beide Fenster.

---

## 4. Ordnerstruktur

**Frontend (`src/`)**

```
main.tsx · App.tsx · index.css
ipc/
├── client.ts                invoke + Event-Bus (onFsProgress/Done/Collision) + Demo
└── bindings/                ts-rs-Typen (DirEntry, DirListing, Drive, OpProgress,
                             OpDone, OpResult, CollisionReq, Preview)
store/
├── panesStore.ts            Seiten mit Tabs, Navigation, Auswahl, Sortierung, Archiv
├── settingsStore.ts         Sprache/Theme/Systemdateien (localStorage-persistiert)
├── clipboardStore.ts        Copy/Cut → Paste (Metadaten)
├── opsStore.ts              Bestätigungs-/Eingabedialoge, Kollisions-Queue, Vorschau, busy
└── transfersStore.ts        laufende Copy/Move/Extract/Pack-Vorgänge (zwei Balken)
i18n/dictionaries.ts         de/en + t()-Mechanismus
features/panel/              Panel · TabBar · DriveSelect · PathBar · StatusBar
                             · FileTable (Table+Virtual) · columns
features/commander/          useKeyboard · navigate · actions · fileOps · preview
                             · selection · FunctionBar
components/ui/               FileContextMenu · SettingsDialog · OperationDialogs
                             · PreviewDialog · TransferView
lib/                         format · fileIcon · cn · path · glob
```

**Backend (`src-tauri/src/`)**

```
main.rs · lib.rs             Bin-Einstieg + Command-Registrierung
commands/fs/
├── mod.rs                   deklariert die Untermodule
├── dir.rs                   list_dir · list_drives · home_dir (+ DirEntry/DirListing/Drive)
├── file.rs                  copy/move/delete/mkdir + Transfer-Infra (Prog, spawn_op,
│                            Kollision, Abbruch) — pub(crate), auch von archive genutzt
├── preview.rs               read_preview (F3) · open_path (F4)
├── archive.rs               Archive (ZIP/tar/tar.gz/tar.xz/7z, Passwort): list_archive ·
│                            extract_entries · create_archive (Packen → ZIP)
└── watch.rs                 set_watched (Verzeichnis-Überwachung, notify)
```

---

## 5. Zustandsmodell (Store)

```ts
type Side = 'left' | 'right';
type SortKey = 'name' | 'ext' | 'size' | 'date';
interface Row extends DirEntry { parent?: boolean }   // parent = ".."-Zeile

interface TabState {
  path: string;
  archive: string | null;        // im ZIP-Browsing: Pfad der .zip, sonst null
  raw: DirEntry[];               // ungefiltert vom Backend
  entries: Row[];                // gefiltert + sortiert (+ "..")
  cursor: number;
  selected: Set<string>;
  anchor: number;                // Ankerzeile für Shift-Bereichsauswahl
  shiftBase: Set<string> | null; // Auswahl-Snapshot während Shift-Sitzung
  sort: { key: SortKey; asc: boolean };
}
interface SideState { tabs: TabState[]; activeTab: number }
interface PanesStore { active: Side; left: SideState; right: SideState; /* …Aktionen */ }
```

**Regeln:** Komponenten abonnieren nur ihren Teilzustand (`panelOf(s, side).cursor`);
Cursor-Bewegung ändert nur `cursor`; Ordner immer vor Dateien; „Systemdateien"-Filter
und `..`-Zeile werden beim Rebuild angewandt.

---

## 6. Tastaturmodell

| Taste | Aktion |
|---|---|
| `Tab` | Aktives Fenster wechseln |
| `↑`/`↓`, `PageUp/Down`, `Home/End` | Cursor bewegen / springen |
| `Enter` | Ordner öffnen · Datei mit Standardprogramm · ZIP betreten |
| `Backspace` | Hoch (bzw. Archiv verlassen) |
| `Insert` | Markieren + Cursor runter (`Space` außerhalb macOS ebenfalls) |
| `Space` | macOS: Quick Look · sonst: Markieren |
| `Shift+↑/↓` | Bereichsauswahl · `Cmd/Ctrl+Klick` Einzel-Toggle · `Shift+Klick` Bereich |
| `+` / `-` / `*` | Muster markieren / abwählen / invertieren |
| Tippen (Buchstabe/Ziffer) | Schnellfilter der aktiven Liste · `Esc` löscht |
| `F3`…`F8` | Ansehen · Bearbeiten · Kopieren · Verschieben · Neuer Ordner · Löschen |
| `Alt+F5` | Packen · `Cmd+C/X/V` Zwischenablage · `Cmd+T/W` Tab auf/zu |

Globaler Listener in `useKeyboard.ts`; Fokus in Eingabefeldern hat Vorrang.

---

## 7. IPC-Verträge

| Command | Args | Rückgabe | Status |
|---|---|---|---|
| `list_dir` | `path` | `DirListing` | ✅ |
| `list_drives` | – | `Drive[]` | ✅ |
| `home_dir` | – | `string` | ✅ |
| `make_dir` | `path` | `string` | ✅ |
| `copy_entries` | `id, sources[], destDir` | – (Events) | ✅ |
| `move_entries` | `id, sources[], destDir` | – (Events) | ✅ |
| `delete_entries` | `paths[]` | `OpResult` | ✅ |
| `cancel_transfer` | `id` | – | ✅ |
| `resolve_collision` | `reqId, action, applyAll` | – | ✅ |
| `read_preview` | `path, maxBytes` | `Preview` | ✅ |
| `open_path` | `path` | – | ✅ |
| `list_archive` | `archive, inner` | `DirListing` | ✅ |
| `extract_entries` | `id, archive, base, names[], destDir` | – (Events) | ✅ |
| `create_archive` | `id, sources[], destZip` | – (Events) | ✅ |
| `set_watched` | `paths[]` | – (Events) | ✅ |

**Events:** `fs-progress` (`OpProgress` – Datei- + Gesamtfortschritt), `fs-done`
(`OpDone` – ok/errors/cancelled), `fs-collision` (`CollisionReq` – blockiert bis Antwort),
`fs-changed` (Ordnerpfad – externe Änderung, löst entprellten Auto-Refresh aus).
Typen werden mit **ts-rs** generiert (`npm run gen:types` → `src/ipc/bindings/`); der
`client.ts`-Wrapper hat einen Demo-Fallback für den Browser ohne Tauri.

---

## 8. Theme & Erscheinungsbild

Farben als CSS-Variablen in `index.css`; Tailwind referenziert sie über
`theme.extend.colors`. **Dunkel** (Default), **Hell** und **System** sind umgesetzt
und über `data-theme` auf `<html>` umschaltbar; „System" folgt `prefers-color-scheme`
live (matchMedia-Listener in `App.tsx`). **Schrift-/Symbolgröße** (Klein/Mittel/Groß)
sind ebenfalls umgesetzt (siehe Einstellungen).

---

## 9. Einstellungen – Kategoriebaum

Der Einstellungsdialog (Radix, links Kategorien, rechts Formular, 1:3–1:1 verschiebbar)
wird in folgende Kategorien gegliedert. Legende: **✅ vorhanden · ⏳ geplant · 💭 fraglich**.

### Allgemein
- Sprache ✅ · Theme Hell/Dunkel ✅ / System ✅
- Schriftart ⏳ · Schriftgröße ✅ · Symbolgröße ✅ · Kompakte Listenansicht ⏳
- Animationen ein/aus 💭 · Fensterposition speichern ✅ · Letzte Tabs wiederherstellen ✅
- Fensteraufteilung merken ✅ *(Splitter verschiebbar, in localStorage persistiert)*

### Favoriten
- Favoriten-Ordner verwalten (hinzufügen/entfernen) ✅
- Erscheinen in der Speicherort-Auswahl über den Laufwerken ✅

### Dateiansicht
- Versteckte/Systemdateien anzeigen ✅ *(heute ein Schalter „Dotfiles"; ggf. in
  „versteckt" + „System" trennen)*
- Ordner zuerst ✅ *(als Option umschaltbar)* · Endungs-Spalte anzeigen/ausblenden ✅
- Größenformat (Automatisch ↔ Bytes) ✅ · Datumsformat ✅ *(TT.MM.JJJJ / TT.MM.JJ / ISO)*
- Berechtigungs-Spalte ✅ *(Unix `rwxr-xr-x` via `mode`; Besitzer-Spalte ⏳)* ·
  Symlinks kennzeichnen ⏳ *(Daten via `is_symlink` da)*
- Spaltenbreiten per Ziehen einstellen ✅ *(persistiert in localStorage)*
- Millisekunden anzeigen 💭 *(Nische)*

### Dateioperationen
- Papierkorb verwenden ✅ *(Standard Papierkorb, Shift = endgültig — ein Schalter)*
- Vor Überschreiben fragen ✅ · Standardaktion bei Konflikten ⏳
- Prüfsummen nach Kopieren ✅ *(SHA-256 Quelle↔Ziel, optional via Einstellung)* ·
  Geschwindigkeitslimit ✅ · Pause/Fortsetzen ✅ · Kopier-Queue ✅ ·
  Parallele Kopien / Threads / Buffergröße ⏳ *(Buffer heute 256 KB fest)*

### Tabs
- Neue Tabs neben aktuellem ⏳ · Beim Beenden speichern ⏳ *(= Session)*
- Tab-Historie ⏳ · Tab-Farbe ⏳ · Tab sperren ⏳

### Vorschau
- Bild ✅ · Text/Hex ✅ · Syntaxhighlighting ✅ · Markdown ✅ · CSV-Tabelle ✅ · PDF ⏳
- RAW↔Vorschau umschaltbar ✅ *(Markdown/CSV/Code/Hex)* · Video-Thumbnail ⏳ · Audioinfos ⏳
  · EXIF ✅ *(Rust `kamadak-exif`)* · Max. Dateigröße ⏳ *(heute 256 KB fest)*

### Suche
- Schnellsuche beim Tippen ⏳ · Standardsuchpfad ⏳ · Verlauf ⏳
- Dateiinhalte durchsuchen ⏳ · Reguläre Ausdrücke ⏳ · Ignorierte Ordner ⏳

### Git
- Git-Status ✅ · Branch ✅ *(Badge in der Pfadzeile)* · An-/Ausschalter ✅
- Remote ⏳ · Farben konfigurieren ⏳ · Diff-Ansicht ⏳

### Netzwerk
- SFTP ⏳ · FTP/FTPS ⏳ · SMB ⏳ · WebDAV ⏳ *(große Baustelle, je Protokoll)*

### Plugins
- Installierte Plugins ⏳ · Updates ⏳ · Vorschau-Plugins ⏳ · Packer-Plugins ⏳
  *(setzt das Plugin-System voraus)*

### Tastatur
- Shortcuts bearbeiten ⏳ · Profile (TC-Modus / Finder-Modus) ⏳

---

## 10. Roadmap & Priorisierung

Priorisierungslogik: **P1** vervollständigt den Kern eines Dateimanagers. **P2** sind die
früh gewünschten Entwickler-/Komfort-Features und macOS-Differenzierung. **P3** ist
fortgeschrittene Dateiarbeit. **P4** sind große Teilsysteme. Querschnittsthemen und eine
„Später/Diskussion"-Liste am Ende.

### ✅ Erledigt
- Tauri-Grundgerüst, Dual-Pane, React+TS-Umbau, ts-rs-Typen
- FileTable (TanStack Table+Virtual), TC-Tastatur, Sortierung
- Auswahl: Shift+Pfeil, Cmd/Ctrl+Klick, `+`/`-`/`*`-Muster
- Tabs pro Fenster (aktiver Tab = Ziel)
- Arbeitsrichtungs-Anzeige auf dem Splitter (Pfeil vom aktiven Fenster zum Zielfenster;
  Kopieren/Verschieben laufen stets in diese Richtung)
- Einstellungs-Modal (Kategoriebaum + verschiebbare Trennung); Sprache, Theme Hell/Dunkel,
  Systemdateien-Filter (localStorage)
- Zwischenablage Copy/Cut/Paste; Kontextmenü (Einfügen nur aktiv bei gefülltem
  Clipboard; Rechtsklick auf Ordner → hinein, auf Datei/Leerraum → aktueller Ordner)
- Dateioperationen am Backend: Copy/Move/Delete/Mkdir/Paste
- Nebenläufige Transfers (Threads) mit Zwei-Balken-Fenster, Minimieren in Statusleiste
- Transfer abbrechen; Kollisionsdialog (Überschreiben/Umbenennen/Überspringen + „für alle")
- Vorschau F3 (Text/Bild/Hex); F4 Öffnen mit Standardprogramm; Umschalt+F4 „Im Editor
  öffnen" (endungsbasiert, konfigurierbar); Enter öffnet Dateien
- Archive browsen & Entpacken (F5): ZIP, tar, tar.gz, tar.xz, 7z — inkl.
  passwortgeschützter ZIP-/7z-Archive; Packen (Alt+F5) erzeugt ZIP
- Rust-Code modular (dir/file/preview/archive/watch)
- Verschiebbarer Fenster-Splitter (persistiert); Verzeichnis-Watcher (Auto-Refresh)
- Favoriten-Ordner (Einstellungen → Speicherort-Auswahl); „In neuem Tab öffnen"
- Ansichtsmodi Detail/Liste/Thumbnails/Baum+Liste (pro Fenster); Commandbar mit
  Sonderfunktions-Icons
- ~~Echte native Fenster statt HTML-Overlays (TICKET-002)~~ — durch TICKET-012
  wieder abgelöst, siehe unten.
- Force-Modal beim Schließen während laufender Transfers (TICKET-004): Schließen
  des Hauptfensters bei aktiven Kopier-/Verschiebe-/Entpack-/Pack-Vorgängen wird
  über `Window.onCloseRequested` abgefangen; Modal erzwingt „Abbrechen & Schließen"
  / „Warten" / „Trotzdem schließen" (`opsStore.forceClose`, `ForceCloseDialog` in
  `OperationDialogs.tsx`). Browser-Demomodus unverändert (No-Op ohne Tauri-Fenster-API).
  *(Weiterhin aktuell — betrifft das Schließen des Hauptfensters, unabhängig von
  der unten abgelösten Mehrfenster-Dialogverwaltung.)*
  - Bugfix: `ForceCloseDialog` schließt über `getCurrentWindow().destroy()`
    (damit `onCloseRequested` nicht erneut greift), aber `capabilities/default.json`
    erlaubte nur `core:window:allow-close`, nicht `allow-destroy` → „Trotzdem
    schließen"/„Abbrechen & Schließen" schlugen still fehl, das Fenster blieb offen.
    `core:window:allow-destroy` ergänzt. Zugleich die `windows`-Liste in den
    Capabilities auf `["main"]` reduziert (die Fenster `settings/rename/export/
    compare/filecompare` existieren seit TICKET-012 nicht mehr).
  - Härtung `onCloseRequested` (App.tsx): blockiert nur noch bei echten aktiven
    Vorgängen — `!done && !cancelled && (queued || fileTotal > 0)`. Ein Transfer
    ohne Fortschritt/Queue (z. B. abgebrochener Passwort-Entpackvorgang, der nie
    `done` wird) hält das rote X nicht mehr dauerhaft fest.
- Bugfix: „Beenden" (Alt+F4, `actions.ts` Aktion `quit`) rief noch das
  Browser-API `window.close()` auf — ein Rest aus der Zeit vor der
  Tauri-Umstellung, das auf dem nativen, vom OS geöffneten Hauptfenster
  wirkungslos ist (kein Effekt, kein Fehler). Jetzt über
  `getCurrentWindow().close()`, wodurch derselbe `onCloseRequested`-Handler
  wie beim nativen Fenster-Button greift (inkl. Transfer-Abfrage oben).
- ~~Echtes modales Verhalten der Dialogfenster über `Window::set_enabled` (TICKET-005)~~
  und ~~Hauptfenster-Freigabe-Härtung (TICKET-011)~~ — beide durch TICKET-012
  gegenstandslos: es gibt keine separaten Dialogfenster mehr, die deaktiviert/
  freigegeben werden müssten.
- Scrollposition/Auswahl nach Dateioperationen bewahren (TICKET-006):
  `reloadBoth()` (nach Copy/Move/Extract/Pack/Delete/Trash) sowie Umbenennen
  und Neuer Ordner nutzen jetzt `panesStore.refresh(side, {reveal:true})`
  statt `loadDir` — Cursor/Auswahl springen nicht mehr auf 0/leer zurück.
  Neu hinzugekommene Einträge (Ziel eines Kopier-/Verschiebe-/Einfüge-
  Vorgangs, umbenannte/neu angelegte Datei) werden automatisch markiert;
  der bestehende `scrollToIndex(…, {align:"auto"})`-Effekt in `FileTable`
  scrollt dabei nur, wenn nötig. Im Browser-Demomodus verifiziert (Cursor/
  Markierung bleiben nach Kopieren exakt erhalten).
- Verzeichnisvergleich startet nicht mehr rekursiv (TICKET-007):
  `CompareDialog.tsx` setzte „Rekursiv" beim Öffnen fälschlich auf `true`
  (Default-State und der erste Vergleichslauf); jetzt startet der Dialog
  immer mit deaktivierter Option, unabhängig vom vorherigen Zustand. Im
  Browser-Demomodus verifiziert (Standard aus → Aktivierung wirkt →
  Schließen/Wiederöffnen setzt zurück).
- Fensterverwaltung auf React-Dialoge umgebaut (TICKET-012): Die separaten
  Tauri-`WebviewWindow`s (Einstellungen/Umbenennen/Export/Vergleich/
  Dateivergleich, TICKET-002) sind vollständig durch In-App-React-Dialoge
  ersetzt — damit sind auch TICKET-005 (macOS-Modalität) und TICKET-011
  (Hauptfenster-Freigabe) gegenstandslos geworden. Neuer zentraler Baustein
  `components/ui/AppDialog.tsx` (Radix `Dialog.Root`, immer zentriert, per
  Ziehgriff vergrößer-/verkleinerbar, geklemmt auf `[Minimum, 90 % Hauptfenster]`,
  reagiert auf Hauptfenster-Resize); Größe pro Dialogtyp persistiert in
  `store/dialogSizeStore.ts` (`zustand` `persist`, unabhängig je `dialogKey`).
  - Bugfix „App reagiert nach etwas Klicken nicht mehr / hängt, obwohl keine
    Operation läuft": `FileBrowserDialog` und `PreviewDialog` verwendeten das
    Muster `if (!x) return null` gefolgt von `<Dialog.Root open>` (fest `true`).
    Beim Schließen unmountete Radix im offenen Zustand und ließ `pointer-events:
    none` (Scroll-/Focus-Lock) auf `<body>` zurück → gesamte App unklickbar.
    Beide Dialoge bleiben jetzt dauerhaft gemountet und werden über
    `open={!!x}` gesteuert (Portal-Inhalt via `{x && (…)}`), sodass Radix den
    `open→closed`-Übergang sieht und die Body-Styles wieder freigibt — analog
    zu den bereits korrekten `AppDialog`-basierten Dialogen und den
    `open={!!…}`-Dialogen in `OperationDialogs.tsx`.
  Alle 5 Dialoge verloren ihre `win`-Prop/Payload-Verzweigung und lesen jetzt
  immer live aus den jeweiligen Stores (kein Unterschied mehr zwischen
  Tauri- und Browser-Demomodus). `useKeyboard.ts` bekam einen zentralen Guard
  (`[role="dialog"]`-Prüfung), damit Tastenkürzel bei offenem Dialog nicht mehr
  an die Dateifenster durchgereicht werden. Entfernt: `commands/window.rs`
  komplett, `on_window_event`-Hook in `lib.rs`, `features/dialogWindow/`,
  `?win=`-Routing in `main.tsx`. Im Browser-Demomodus vollständig verifiziert
  (alle 5 Dialoge einzeln + gleichzeitig, Resize, Persistenz, Esc, Fokus-
  Wiederherstellung, Tastatur-Isolation); kein Live-Test gegen eine echte
  Tauri-App (unkritisch, da keine Tauri-spezifische Logik mehr existiert).
- Auswahl im inaktiven Panel sichtbar gemacht (TICKET-008): Die Cursor-Zeile
  im inaktiven Fenster hatte bisher nur einen 1px-Outline ohne Füllung und
  war dadurch kaum erkennbar. Neue CSS-Variable `--cursor-inactive-marker`
  (`rgba(154, 194, 123, 0.4)`, beide Themes) füllt die Cursor-Zeile im
  inaktiven Panel jetzt dezent grün (`FileTable.tsx`s `stateClass()`); bei
  zusätzlicher Markierung bleibt die gelbe Auswahlfarbe dominant. Im
  Browser-Demomodus in Dark- und Light-Theme verifiziert.

### P1 — Kern vervollständigen
- [x] **Umbenennen (F2)** einzelner Einträge (auch im Kontextmenü; Dialog wählt den
      Basisnamen ohne Endung vor, prüft Namenskonflikte; Backend `rename_entry`)
- [x] **Papierkorb** statt endgültig löschen (Shift = endgültig; Backend `trash`-Crate;
      Schalter „Papierkorb verwenden" unter Einstellungen → Dateioperationen; eigene
      Dialoge: sanft „In den Papierkorb" vs. rot „Endgültig löschen")
- [x] **Verschiebbarer Fenster-Splitter** (persistiert)
- [x] **Verzeichnis-Watcher** (`notify`): externe Änderungen lösen entprellten,
      cursor-schonenden Auto-Refresh aus
- [x] **Ansichtsmodi**: Detail / Liste / Thumbnails / Treeview mit Liste (pro Fenster
      umschaltbar via Commandbar; Listenansicht dient zugleich als kompakte Ansicht).
      Treeview: Baum an der Laufwerks-/Home-Wurzel, auto-aufgeklappt zum aktuellen Pfad,
      lazy geladen; Einfachklick wählt aus, Doppelklick/Enter lädt in die Liste
- [x] **Commandbar** mit Icons für Sonderfunktionen (Aktualisieren, Hoch, Neuer Ordner,
      Ansehen, Packen, Einstellungen) + Ansichtsumschalter für das aktive Fenster
- [x] **Breadcrumb-Navigation** in der Pfadzeile (klickbare Segmente springen in den
      jeweiligen Ordner; funktioniert auch innerhalb von Archiven)
- [x] **Archiv sichtbar machen**: amber Badge „📦 archiv.zip ›" in der Pfadzeile (links
      fixiert, immer sichtbar; Klick springt zur Archiv-Wurzel) + Paket-Icon und Archivname
      im Tab
- [x] **Einstellungs-Ausbau**: Kategorien vervollständigt (Dateiansicht: Ordner zuerst,
      Endungs-Spalte, Größen-/Datumsformat, Rechte-Spalte; Theme „System";
      Schrift-/Symbolgröße). Besitzer-Spalte bewusst zurückgestellt.
- [x] **Spaltenbreiten per Ziehen** in der Detailansicht anpassbar (Griff am linken
      Spaltenrand; Breiten in localStorage persistiert)
- [x] **Copy Path / Copy Filename** (Kontextmenü; „Pfad kopieren" = absoluter Pfad,
      „Dateiname kopieren" = nur der Name; Mehrfachauswahl zeilenweise)
- [x] **Favoriten-Ordner** (Einstellungen) — erscheinen wie „Laufwerke"
- [x] **Ordner „In neuem Tab öffnen"** (Kontextmenü)
- [x] **Zuletzt besuchte Ordner / Verlauf**: Zurück/Vor pro Fenster (Alt+←/→ + Toolbar)
      und globale, persistierte Liste zuletzt besuchter Ordner (Verlauf-Dropdown in der
      Commandbar, Sprung per Klick, „Verlauf leeren")
- [x] **Session/Workspace**: letzte Tabs beider Fenster (Pfade, aktiver Tab, Ansichtsmodus)
      werden in localStorage gesichert und beim Start wiederhergestellt (`sessionStore` +
      `panesStore.restoreSession`); Fensterposition/-größe via `lib/windowState.ts`
      (`@tauri-apps/api/window`). Nicht mehr vorhandene Ordner fallen auf Home zurück.

### P2 — Entwickler & Komfort (früh gewünscht) + macOS
- [~] **Git-Integration**: Basis umgesetzt — erkennt Git-Repos (Branch-Badge in der
      Pfadzeile) und färbt Einträge nach Git-Status (modified/new/deleted/renamed/
      ignored), in den Einstellungen ab-/anschaltbar. Backend `git_status_watch` via
      git-CLI, läuft **asynchron über `spawn_blocking`** und liefert das Ergebnis per
      Event `git-support-ready` (blockiert weder Ordneröffnung noch UI, auch nicht bei
      großen Repos; TICKET-003).
      Offen: konfigurierbare Farben, Diff-Ansicht, Status im Baum/Tab
- [x] **Terminal im aktuellen Ordner** (Kontextmenü „Im Terminal öffnen"; Standard-Terminal
      in den Einstellungen konfigurierbar, leer = System-Standard) + „Im Dateimanager öffnen"
- [x] **Datei im Editor öffnen** (konfigurierbarer Editor, Alternative zu F4): Programm je
      Endung zuordenbar (zwei Sichten „Nach Endung"/„Nach Programm") + globaler Standard-
      Editor; Auslösen wählbar (Umschalt+F4 / nur Kontextmenü / F4). Programme werden über
      den **wiederverwendbaren Dateibrowser-Modal** ausgewählt. Backend `open_with`.
- [x] **Wiederverwendbarer Dateibrowser-Modal** (`fileBrowserStore` + `FileBrowserDialog`,
      `selectMode` file/folder/any) — von überall im Programm aufrufbar
- [x] **Quick Look** (Leertaste, macOS) — native Vorschau via `qlmanage -p` (Backend
      `quick_look`); Space löst auf macOS Quick Look aus (Markieren dort über Einfg),
      sonst unverändert Markieren. Zusätzlich Kontextmenüpunkt „Quick Look" (nur macOS).
- [x] **Schnellfilter** (Filtern beim Tippen): Tippen filtert die aktive Liste per
      Namens-Teilstring (Cursor springt auf ersten Treffer); Backspace bearbeitet, Esc löscht,
      Enter öffnet den Treffer. Filterleiste mit Trefferzahl; per Tab gespeichert, beim
      Navigieren zurückgesetzt. `+`/`-`/`*` bleiben Muster-Auswahl, Space bleibt Quick Look.
- [x] Vorschau erweitern: **EXIF** im Bild (Rust `kamadak-exif`), **Markdown** (marked +
      DOMPurify), **Syntaxhighlighting** (highlight.js), **CSV-Tabelle**; RAW↔Vorschau je Format
      umschaltbar, Binär Hex↔Text. HTML bewusst nur hervorgehoben (kein Live-Render).
- [x] **Dateilisten exportieren**: Command-Button (Icon) im aktiven Fenster öffnet einen
      Konfigurationsdialog mit Live-Vorschau. Umfang (ganze Liste / nur markierte), Format
      (Text-Liste, CSV, TSV, JSON, XML), wählbare Felder (voller Pfad – Standard, Name,
      Endung, Größe, Datum, Typ, Rechte) und Optionen (Ordner einschließen, CSV/TSV-Kopfzeile,
      Größe/Datum formatiert vs. Rohwerte). Ergebnis speichern (Backend `write_text_file`,
      Zielordner über den wiederverwendbaren Dateibrowser) oder in die Zwischenablage.
      Beim Speichern auf eine vorhandene Datei erfolgt eine Kollisionsabfrage
      (Überschreiben bestätigen) über den vorhandenen Bestätigungsdialog. Reine
      Aufbereitung im Frontend (`features/commander/exportList.ts`), Schreiben im Backend.
- [x] **Finder-Tags** anzeigen/bearbeiten (macOS): Abschnitt „Finder-Tags" im
      Eigenschaften-Dialog (`PropertiesDialog.tsx`, nur macOS via `isMacOS`).
      Tags werden als farbige Chips angezeigt (× zum Entfernen), Hinzufügen über
      Namensfeld + Farbwahl (Finder-Farben 0–7). Backend `commands/fs/tags.rs`
      (`get_tags`/`set_tags`) liest/schreibt das xattr
      `com.apple.metadata:_kMDItemUserTags` als **Binär-plist** (`plist`-Crate,
      auf macOS beschränkt); leere Liste entfernt das Attribut. Nicht-macOS liefert
      leere Werte bzw. einen klaren Fehler.

### P3 — Fortgeschrittene Dateiarbeit
- [x] **Verzeichnisvergleich/-synchronisation**: Command-Button (Icon) öffnet einen
      Dialog, der das linke gegen das rechte Fenster vergleicht (nach Größe +
      Änderungszeit, Symlinks werden übersprungen). Formular oben (beide Pfade,
      Neu-vergleichen, **umschaltbar rekursiv** = mit/ohne Unterordner, Filter
      Unterschiede/Nur-links/Nur-rechts/Gleiche mit Zählern), Liste unten in voller
      Breite mit Status je Datei (gleich, links/rechts neuer, unterschiedlich, nur
      links/rechts) samt Größen; Fenster mausgrößenveränderbar. Auswahl per
      Zeilen-Checkbox + „alle sichtbaren"; **links↔rechts kopieren** über
      richtungsabhängige Buttons (Quelle muss vorhanden sein), danach automatisch neu
      verglichen und Fenster aktualisiert. Backend `compare_dirs` + `sync_copy`
      (`commands/fs/compare.rs`) laufen **asynchron über `spawn_blocking`**, damit
      große Bäume die UI nicht blockieren; Zielordner werden beim Kopieren angelegt.
      Der Vergleich **streamt die Ergebnisse pro Verzeichnis** über einen Tauri-`Channel`
      (`on_batch`): jede gelesene Ebene wird sofort ans Frontend geschickt und dort an
      die Liste angehängt (mit Lauf-Token gegen veraltete Chargen), sodass bei großen
      Bäumen nicht auf das Gesamtergebnis gewartet werden muss. Die Ergebnisliste ist
      **virtualisiert** (`@tanstack/react-virtual`) – nur die sichtbaren Zeilen sind im
      DOM (z. B. ~30 statt >1000). Hartes **Limit von 200.000 Dateien**: der Walk bricht
      dann ab und das Backend meldet die Kürzung zurück; der Dialog zeigt einen
      Warnhinweis („Ergebnis verkleinern").
- [x] **Datei-/Binärvergleich**: Vergleicht die Cursor-Datei des linken gegen die
      des rechten Fensters (Commandbar-Button `GitCompare` bzw. Kontext „immer links
      gegen rechts"; `FileCompareDialog.tsx` ↔ Backend `commands/fs/filecompare.rs`,
      `compare_files`). Backend erkennt Text vs. Binär (NUL-Byte/ungültiges UTF-8),
      liefert im Textmodus einen zeilenweisen Diff (similar/Myers, side-by-side mit
      equal/replace/delete/insert-Farben) und im Binärmodus eine Hex-Gegenüberstellung
      (16 Byte/Zeile, differierende Zeilen markiert). Byte-Identität wird streamend
      geprüft. Läuft **asynchron über `spawn_blocking`**; Ergebnis virtualisiert
      (`@tanstack/react-virtual`). Harte Obergrenzen (4 MB Text/Seite, 200.000 Diff-
      Zeilen, 1 MB Hex/Seite) mit „gekürzt"-Hinweis. (Inline-Wort-Diff: offen.)
- [x] **Massenumbenennen** (Muster/Regex, Vorschau): Command-Button (Icon) im aktiven
      Fenster öffnet einen Dialog mit Live-Vorschau (alt → neu) und Konflikterkennung.
      Umfang ganze Liste / nur markierte; Namensschema mit Platzhaltern (`[N]` Name,
      `[E]` Endung, `[C]` Zähler mit Start/Schritt/Stellen); Suchen & Ersetzen mit
      optionalem Regex ($1 …) und Groß-/Kleinschreibung; Groß-/Klein-Umwandlung
      (unverändert/klein/GROSS/Erster groß). Konflikte (leerer/ungültiger Name, doppelte
      Zielnamen, fremdbelegtes Ziel) werden rot markiert und ausgelassen. Ausführung im
      Backend (`rename_batch`, Zwei-Phasen über temporäre Namen → auch Namenstausch/-ketten
      sicher). Namensberechnung im Frontend (`features/commander/renameRules.ts`).
- [~] **Undo für Dateioperationen** ⏳; **Kopier-Queue** ✅; **Hintergrundjobs** ✅;
      **Pause/Fortsetzen** ✅; **Geschwindigkeitsbegrenzung** ✅; **Prüfsummen** ✅.
      Transfer-Engine ausgebaut (`commands/fs/file.rs`): **Pause/Fortsetzen** über ein
      Pausen-Flag je Vorgang (`pause_transfer`, Loop wartet und meldet „paused"-Ticks);
      **Geschwindigkeitslimit** (Bytes/s, Fenster-Drosselung) und **Prüfsummen-
      Verifikation** (SHA-256 Quelle vs. Ziel nach dem Kopieren) als Einstellungen
      (`settings.speed`, `settings.verify`); **Kopier-Queue** serialisiert Vorgänge
      im Frontend (`fileOps.ts`: `startNextQueued`/`dequeueTransfer`, `settings.queue`),
      wartende zeigen „Wartet …". Hintergrundjobs/Minimieren/Abbrechen bestanden bereits.
      Transfer-Fenster: Pause/Fortsetzen-Button, „Pausiert"/„Wartet"-Status.
      **Undo bewusst zurückgestellt** (Datensicherheit: nur beweisbar umkehrbare
      Operationen; eigener sorgfältiger Schritt).
- [x] **Weitere Archivformate**: Browsen & Entpacken für tar, tar.gz, tar.xz und 7z
      (Format über Endung erkannt, `lib/archive.ts` ↔ `archive_kind`); passwortgeschützte
      ZIP- (AES/ZipCrypto) und 7z-Archive: Passwort-Sentinels (`PASSWORD_REQUIRED`/
      `PASSWORD_WRONG`) aus dem Backend, Frontend fragt maskiert nach, zwischengespeichert
      pro Archiv (`archivePw.ts`) und automatisch beim Entpacken wiederverwendet. Packen
      erzeugt weiterhin ZIP. (Verschlüsseltes/format-übergreifendes Packen: offen.)
- [x] **Suche**: Such-Dialog (Commandbar-Button + **Alt+F7**; `SearchDialog.tsx` ↔
      Backend `commands/fs/search.rs`, Command `search`) mit vier Modi:
      **Dateien** (Name als Glob/Regex + **Dateiinhalte** als Text/Regex,
      Groß-/Kleinschreibung), **Duplicate Finder** (Größe → SHA-256, Gruppen farbig),
      **leere Ordner**, **große Dateien** (ab Mindestgröße, nach Größe sortiert).
      **Ignorierte Ordner** (Standard: node_modules/.git/target/.cache/dist).
      Läuft **asynchron über `spawn_blocking`** und **streamt Treffer** über einen
      Tauri-`Channel`; Ergebnisliste **virtualisiert** (`@tanstack/react-virtual`),
      Doppelklick zeigt den Treffer im aktiven Fenster (Reveal). Hartes Trefferlimit
      (50.000) mit „gekürzt"-Hinweis; Inhaltssuche bis 8 MB/Datei, binäre übersprungen.
- [x] **Rechte**: Eigenschaften-Dialog (Kontextmenü „Eigenschaften …" / Alt+Enter,
      `PropertiesDialog.tsx` ↔ Backend `commands/fs/props.rs`). Zugriffsrechte
      anzeigen + bearbeiten (chmod über rwx-Matrix, setuid/setgid/sticky, Oktalfeld;
      `set_permissions`), Besitzer/Gruppe anzeigen und ändern (chown mit Name oder
      ID; `set_owner`), Extended Attributes und ACL anzeigen (`xattr`/`exacl`),
      Prüfsummen MD5/SHA-1/SHA-256 auf Knopfdruck (`file_checksums`). Unix-Teile
      (Besitzer/xattr/ACL) hinter `cfg(unix)`; Windows liefert leere/None-Werte.
- [x] **Parallele Kopien / Thread-Anzahl / Buffergröße** (Settings + Backend):
      Drei neue Einstellungen (`settings.buffer`, `settings.threads`; Puffergröße
      Standard 256 KB, Thread-Anzahl Standard **1 = sequenziell/unverändert**).
      Buffergröße wird durch den Kopierpfad gereicht (`resolve_buf`, 4 KB–16 MB).
      **Parallele Kopien** über einen bewusst **isolierten Pfad** (`run_parallel_copy`
      + `SharedProg` mit atomaren Zählern, `commands/fs/file.rs`), der nur beim
      reinen **Kopieren** und Thread-Anzahl > 1 aktiv wird; Kollisionen werden vorab
      sequenziell/interaktiv aufgelöst, dann werden die Dateiaufgaben über N Worker
      abgearbeitet. Pause, Abbruch, Prüfsummen und ein **globales** Geschwindigkeits-
      limit gelten auch parallel. **Move bleibt sequentiell** (rename ist sofort;
      Copy-Fallback selten) und der bestehende Ein-Thread-Pfad ist unverändert.

### P4 — Große Teilsysteme
- [ ] **Netzwerk** (identische Oberfläche): **SFTP zuerst**, dann SMB, WebDAV, FTP/FTPS;
      SSH-Terminal
      *(aufgeschlüsselt in Ticketserie `tickets/020`–`026`: 020 VFS-Abstraktion
      (Fundament) → 021 Verbindungen/Zugangsdaten (OS-Keychain) → 022 SFTP → 023 SMB,
      024 WebDAV, 025 FTP/FTPS → 026 SSH-Terminal; 027 optionales KeePass-Backend.
      „Identische Oberfläche" via Provider-Trait + Schema-Registry; Geheimnisse per
      `CredentialStore` im OS-Keychain (KeePass optional))*
- [ ] **Plugin-System**: **API früh definieren**, dann Rust-/JS-Plugins,
      Kontextmenü-/Vorschau-/Spalten-/Archiv-Erweiterungen, Skripting (JS)
      *(aufgeschlüsselt in Ticketserie `tickets/100`–`107`: 100 API/Manifest-Vertrag →
      101 JS-Host, 102 nativer Rust-Host → 103 Plugin-Commandbar/bunte Icons →
      104 Kontext-Übergabe → 105 vermittelte Aktionen/Events → 106 Verwaltung →
      107 Demo-Plugin. Broker als einzige Vermittlung, default-deny-Rechte; JS
      sandboxbar, native Plugins vertrauensbasiert)*
- [ ] **Mehrspalten-Ansicht** (2–4 Panels), mehrere Fenster
- [ ] **Spotlight-Suche** integrieren (macOS)

### Querschnitt — Performance
- [ ] Verzeichnis-Watcher (P1) · Directory-Cache · Thumbnail-Cache · Hintergrundindex
      (mit Suche) · Lazy Loading *(Virtual bereits vorhanden)*

### 💭 Später / zu klären
- **RAR lesen** — Lizenzproblem (`unrar` unfrei); nur über System-Tool oder weglassen
- **Drei-Wege-Vergleich**, **Millisekunden anzeigen**, **AppleScript** — Nische, niedrige Prio
- **Plugin-Marktplatz** — erst nach stabiler Plugin-API sinnvoll
- **iCloud-Status / Volumes sauber darstellen** — macOS-Feinschliff, an Finder-Integration koppeln

> **Hinweis zur langfristigen Vision:** Das Alleinstellungsmerkmal entsteht aus der
> Kombination *Git-Integration + integrierte Vorschau (Bild/PDF/Markdown) + Terminal im
> Panel + SFTP mit identischer Oberfläche + Plugins*. Diese Achsen (P2→P4) sollten die
> Architektur-Entscheidungen leiten – insbesondere die **Plugin-API früh** mitdenken.

---

## 11. Herkunft der Backlog-Punkte (Nachverfolgung)

Beim Priorisieren wurde die frühere, unsortierte Roadmap in §10 einsortiert. Die
folgende Tabelle zeigt „alt → neu", damit nachvollziehbar bleibt, wo jeder Punkt
gelandet ist (es ging nichts verloren).

| Früherer Roadmap-Punkt | Jetzt in |
|---|---|
| Umbenennen (F2) | **P1** – „Umbenennen (F2)" |
| Fensteraufteilung verschiebbar (aktuell fix) | **P1** – „Verschiebbarer Fenster-Splitter" |
| Verzeichnisse überwachen & anzeigen | **P1** – „Verzeichnis-Watcher" (+ Querschnitt Performance) |
| Commandbar mit Icon für Sonderfunktionen | **P1** – „Commandbar mit Icons" |
| Ansichtsmodi Detail/Liste/Thumbnail (Icon) | **P1** – „Ansichtsmodi" |
| Visualisieren, dass man im Archiv ist | **P1** – „Archiv sichtbar machen (Badge)" |
| Konfiguration/Einstellungen ausbauen | **P1** – „Einstellungs-Ausbau" (+ §9 Kategoriebaum) |
| Favoriten-Ordner, als Laufwerk angezeigt | **P1** – „Favoriten-Ordner" |
| Verlauf | **P1** – „Zuletzt besuchte Ordner / Verlauf" |
| GIT im Ordner farblich, konfigurierbar | **P2** – „Git-Integration" (+ §9 Git-Kategorie) |
| Bildvorschau zeigt EXIF | **P2** – „Vorschau erweitern: EXIF …" |
| Dateilisten als Text/CSV exportieren | **P2** – „Dateilisten exportieren" |
| Verzeichnisse vergleichen/synchronisieren | **P3** – „Verzeichnisvergleich/-synchronisation" |
| Massenumbenennen | **P3** – „Massenumbenennen" |
| ACL/Berechtigungen/Besitzer | **P3** – „Rechte: ACL …" ✅ |
| Undo/Queue/Hintergrundjobs/Speed/Pause | **P3** – „Undo … Kopier-Queue … Pause/Fortsetzen …" |
| Weitere Archivformate (tar/7z) | **P3** – „Weitere Archivformate" ✅ (Lesen/Entpacken + Passwort) |

**Weitere Einsortierungen ohne eigenen P-Punkt:**
- *„Tab-Ordner"* → §9 › **Tabs** (Tab-Historie, Tab sperren, Tab-Farbe).
- *„Vor Überschreiben fragen"* → bereits erledigt (Kollisionsdialog), im ✅-Block bzw. §9 mit ✅.

**Bewusste Deprioritisierung / Klärung** (siehe §10 „Später / zu klären"):
RAR lesen (Lizenz), Millisekunden anzeigen, Drei-Wege-Vergleich, AppleScript,
Plugin-Marktplatz.
