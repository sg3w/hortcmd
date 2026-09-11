# Menu ↔ command inventory (TICKET-058)

Baseline commit: `b8bbec8` ("TICKET-040: Commander menu in the window + reduced
native macOS menu"). Every row was checked against the running code, not against
the roadmap: a backend command or a ticked ticket alone does not count — dialog,
execution and error path had to exist.

Legend for **Status**:

| Mark | Meaning |
|---|---|
| `040` | Already wired by TICKET-040; re-checked here, unchanged |
| `058` | Wired (or corrected) by this ticket |
| `dialog` | Deliberately reachable only inside a dialog, not as a menu entry |
| `059` | Belongs to the Mark menu, owned by TICKET-059 |
| `open` | Implemented nowhere yet — no menu entry, by design |

Availability column: `state()` of the command in `features/commander/commands.ts`.

---

## Dateien / Files

| Function | Implementation | Command id | Menu path | Shortcut | Context | Availability | Status |
|---|---|---|---|---|---|---|---|
| Open entry | `navigate.ts:openEntry` | `file.open` | Dateien › Öffnen | Enter | cursor | folder reachable, list not empty | 040 |
| View (F3) | `preview.ts:runView` | `file.view` | Dateien › Anzeigen | F3 | cursor | cursor on a file | 040 |
| Edit (F4) | `fileOps.ts:runEdit` / `runEditWith` | `file.edit` | Dateien › Bearbeiten | F4 | cursor | not in archive, cursor on a file | 040 |
| Open with a configured program | `fileOps.ts:runOpenWithProgram` | dyn. `prog:*` | Dateien › Öffnen mit ▸ | — | cursor | like `file.edit` | 040 |
| Open with the default editor | `settingsStore.defaultEditor` | dyn. `prog:default` | Dateien › Öffnen mit ▸ | — | cursor | like `file.edit` | 040 |
| **Open with another program** | `fileOps.ts:runOpenWithBrowse` | `file.openWithOther` | Dateien › Öffnen mit ▸ | — | cursor | not in archive, cursor on a file | **058** |
| Manage programs | `SettingsDialog:ProgramsForm` | `file.programs` | Dateien › Öffnen mit ▸ | — | global | always | 040 (058: opens the *Öffnen mit* page directly) |
| Copy (F5) | `fileOps.ts:runCopy` | `file.copy` | Dateien › Kopieren | F5 | selection/cursor | ≥ 1 target | 040 |
| Move (F6) | `fileOps.ts:runMove` | `file.move` | Dateien › Verschieben | F6 | selection/cursor | not in archive, ≥ 1 target | 040 |
| Rename (F2) | `fileOps.ts:runRename` | `file.rename` | Dateien › Umbenennen | F2 | selection/cursor | not in archive, ≥ 1 target | 040 |
| New folder (F7) | `fileOps.ts:runMkdir` | `file.mkdir` | Dateien › Neuer Ordner | F7 | active folder | not in archive | 040 |
| Delete → trash | `fileOps.ts:runDelete` | `file.delete` | Dateien › Löschen | F8 | selection/cursor | not in archive, ≥ 1 target | 040 |
| Delete permanently | `fileOps.ts:runDelete({permanent})` | `file.deletePerm` | Dateien › Endgültig löschen | ⇧F8 | selection/cursor | not in archive, ≥ 1 target | 040 |
| Clipboard copy / cut / paste | `actions.ts` `clip-*`, `fileOps.ts:runPaste` | `file.clipCopy` / `clipCut` / `clipPaste` | Dateien | ⌘/Ctrl C · X · V | selection/cursor, clipboard | paste needs a filled clipboard | 040 |
| Pack | `fileOps.ts:runPack` | `file.pack` | Dateien › Packen | ⌥F5 | selection/cursor | not in archive, ≥ 1 target | 040 |
| Extract | `fileOps.ts:runExtractArchive` | `file.extract` | Dateien › Entpacken | — | selection/cursor | not in archive, ≥ 1 target | 040 |
| Batch rename | `renameStore.openRename` | `file.batchRename` | Dateien › Mehrfach umbenennen | — | selection/cursor | not in archive, ≥ 1 target | 040 |
| Compare files by content | `fileCompareStore.openFileCompare` | `file.compareFiles` | Dateien › Dateien nach Inhalt vergleichen | — | both panes | one file under each cursor | 040 |
| Properties (rights, tags, checksums, size) | `propsStore.openProps`, `PropertiesDialog` | `file.props` | Dateien › Eigenschaften | ⌥⏎ | cursor | not in archive, cursor on an entry | 040 |
| Native system properties | `client.ts:openSystemProperties` (TICKET-015) | — | button inside `PropertiesDialog` | — | — | — | dialog |
| Permissions / owner / tags editing | `setPermissions`, `setOwner`, `setTags` | — | inside `PropertiesDialog` | — | — | — | dialog |
| Quit | `actions.ts:quit` | `app.quit` | Dateien › Beenden | Alt+F4 | global | hidden on macOS (native app menu owns ⌘Q) | 040 |
| File undo | — (TICKET-048) | — | — | — | — | — | open |

## Markieren / Select

TICKET-059 owns this menu in full and implements its missing functions. Wired
today (all from 040, re-checked, untouched here):

| Function | Implementation | Command id | Shortcut | Status |
|---|---|---|---|---|
| Select group by pattern | `selection.ts:promptSelect(true)` | `mark.selectGroup` | `+` | 040 |
| Unselect group by pattern | `selection.ts:promptSelect(false)` | `mark.unselectGroup` | `-` | 040 |
| Invert selection | `selection.ts:invertSelection` | `mark.invert` | `*` | 040 |
| Select all visible / none | `panesStore.selectPattern("*", …)` | `mark.all` / `mark.none` | — | 040 |
| Copy file names / full paths | `fileOps.targetPaths` + `lib/clipboard` | `mark.copyNames` / `mark.copyPaths` | — | 040 |
| Export file list | `exportStore.openExport`, `exportList.ts` | `mark.export` | — | 040 |
| Mark entry under the cursor | `panesStore.toggleMark` | — | Insert / Space | 059 |
| Same extension · remember/restore · save/load selection · detailed clipboard variants · compare-and-mark | — | — | — | 059 |

## Befehle / Commands

| Function | Implementation | Command id | Menu path | Shortcut | Availability | Status |
|---|---|---|---|---|---|---|
| Search (files) | `searchStore.openSearch`, `SearchDialog` | `cmd.search` | Befehle › Suchen | ⌥F7 | not in archive | 040 |
| **Search: duplicates** | `SearchDialog` mode `duplicates` | `cmd.searchDuplicates` | Befehle › Spezialsuche ▸ | — | not in archive | **058** |
| **Search: empty folders** | `SearchDialog` mode `empty_dirs` | `cmd.searchEmptyDirs` | Befehle › Spezialsuche ▸ | — | not in archive | **058** |
| **Search: large files** | `SearchDialog` mode `large_files` | `cmd.searchLargeFiles` | Befehle › Spezialsuche ▸ | — | not in archive | **058** |
| Compare / synchronize directories | `compareStore.openCompare`, `CompareDialog` | `cmd.compareDirs` | Befehle | — | always | 040 |
| Synchronizing copy | `client.ts:syncCopy` | — | inside `CompareDialog` | — | — | dialog |
| Back / forward / parent | `panesStore.goBack/goForward`, `openEntry` | `nav.back` / `nav.forward` / `nav.up` | Befehle › Navigation ▸ | ⌥← · ⌥→ · ⌫ | history / `..` row present | 040 |
| Favorites | `settingsStore.favorites` | dyn. `fav:*` | Befehle › Favoriten ▸ | — | empty list → placeholder | 040 |
| Recent folders + clear | `historyStore` | dyn. `recent:*`, `recent.clear` | Befehle › Zuletzt besuchte Ordner ▸ | — | clear disabled when empty | 040 |
| Terminal in the current folder | `client.ts:openTerminal`, `terminalStore.openFor` | `cmd.terminal` | Befehle | — | local or SFTP; other protocols have no shell | 040 |
| Open in the system file manager | `client.ts:openPath` | `cmd.openFolder` | Befehle | — | local paths only | 040 |
| Running / queued transfers | `transfersStore`, `TransferView` | dyn. `transfer:*` | Befehle › Übertragungen ▸ | — | empty list → placeholder | 040 |
| Pause / resume / cancel a transfer | `transfersStore.setPaused`, `cancelTransfer` | — | inside the transfer dialog | — | — | dialog |
| Drive / volume switch | `panel/DriveSelect.tsx` | — | per-pane location picker (also lists favorites and connections) | — | — | dialog |
| Storage usage analysis | — (TICKET-056) | — | — | — | — | open |
| Plugin management | — (TICKET-106) | — | — | — | — | open |

## Netz / Network

| Function | Implementation | Command id | Menu path | Availability | Status |
|---|---|---|---|---|---|
| Manage connections (all protocols) | `netStore.openManager`, `ConnectionsDialog` | `net.manage` | Netz | always, also without profiles (TICKET-016) | 040 |
| **Connect to a saved profile** | `netStore.connect` + `panesStore.loadDir` | dyn. `profile:*` | Netz › Verbinden mit ▸ | only profiles that are not connected; empty list → placeholder | **058** |
| Open an active connection in the pane | `netStore.connections` + `loadDir` | dyn. `conn:*` | Netz › Aktive Verbindungen ▸ | empty list → placeholder | 040 |
| Disconnect the current connection | `netStore.disconnect` | `net.disconnect` | Netz | only with a live connection for this folder | 040 |
| SSH terminal | `terminalStore.openFor` | `net.terminal` | Netz | SFTP only, connection live | 040 |
| Create / edit / delete a profile, store the secret | `ConnectionsDialog`, `netStore.save/remove` | — | inside the connection manager | — | dialog |
| Close the SSH terminal | `terminalStore.close` | — | close button on the terminal panel | — | dialog |

SFTP, SMB, WebDAV and FTP/FTPS share the same profile management, so they need no
protocol-specific menu entries.

## Ansicht / View

| Function | Implementation | Command id | Menu path | Shortcut | Status |
|---|---|---|---|---|---|
| Details / list / thumbnails / tree | `panesStore.setViewMode` | `view.details` … `view.tree` (radio) | Ansicht › Darstellung ▸ | — | 040 |
| Sort key name/ext/size/date | `panesStore.setSortKey` | `sort.name` … `sort.date` (radio) | Ansicht › Sortieren nach ▸ | — | 040 |
| Sort direction | `panesStore.setSortAsc` | `sort.asc` / `sort.desc` (radio) | Ansicht › Sortieren nach ▸ | — | 040 |
| Show hidden / system files | `settingsStore.setHideSystemFiles` + `panesStore.rebuildAll` | `view.hidden` (check) | Ansicht | — | 040 — **058 added the missing `rebuildAll`**, the checkbox used to flip without changing the list |
| **Sort folders first** | `settingsStore.setFoldersFirst` + `rebuildAll` | `view.foldersFirst` (check) | Ansicht | — | **058** |
| Extension column | `settingsStore.setShowExtColumn` | `view.extCol` (check) | Ansicht | — | 040 (no rebuild needed — column visibility only) |
| Permissions column | `settingsStore.setShowPermissions` | `view.permCol` (check) | Ansicht | — | 040 (same) |
| Git status | `settingsStore.setGitEnabled` + `panesStore.reloadGit` | `view.git` (check) | Ansicht | — | 040 — **058 added the missing `reloadGit`** |
| Reset quick filter | `panesStore.setFilter(side, "")` | `view.clearFilter` | Ansicht | Esc | 040 |
| New / close tab, folder under cursor in a new tab | `panesStore.addTab/closeTab`, `actions.ts:open-tab` | `view.newTab` / `view.closeTab` / `view.openTab` | Ansicht | ⌘T · ⌘W · — | 040 |
| Refresh the active pane | `panesStore.refresh` | `view.refresh` | Ansicht | — | 040 |
| Quick Look | `fileOps.ts:runQuickLook` | `view.quickLook` | Ansicht | Space | 040 — hidden off macOS |
| Column widths, pane splitter, tab strip | `columns.tsx`, `App.tsx`, `TabBar.tsx` | — | drag interactions, no menu entry | — | dialog |

## Konfiguration / Configuration

| Function | Implementation | Command id | Menu path | Shortcut | Status |
|---|---|---|---|---|---|
| Settings | `uiStore.openSettings`, `SettingsDialog` | `cfg.settings` | Konfiguration | ⌘, / Ctrl+, | 040 |
| **Settings page: Allgemein** (language, theme, terminal program, font/icon size) | `SettingsDialog:GeneralForm` | `cfg.general` | Konfiguration › Einstellungsseite öffnen ▸ | — | **058** |
| **Settings page: Dateiansicht** (folders first, columns, size/date format) | `ViewForm` | `cfg.view` | ▸ | — | **058** |
| **Settings page: Dateifarben** | `ColorsForm` (TICKET-009) | `cfg.colors` | ▸ | — | **058** |
| **Settings page: Dateien** (hidden files, Git) | `FilesForm` | `cfg.files` | ▸ | — | **058** |
| **Settings page: Dateioperationen** (trash, verify, speed limit, queue, buffer, threads) | `OperationsForm` | `cfg.operations` | ▸ | — | **058** |
| **Settings page: Öffnen mit** (programs, associations, default editor) | `ProgramsForm` | `cfg.programs` | ▸ | — | **058** |
| **Settings page: Favoriten** | `FavoritesForm` | `cfg.favorites` | ▸ | — | **058** |
| Appearance system/light/dark | `settingsStore.setTheme` | `theme.*` (radio) | Konfiguration › Erscheinungsbild ▸ | — | 040 |
| Language de/en | `settingsStore.setLanguage` | `lang.*` (radio) | Konfiguration › Sprache ▸ | — | 040 |

No "save settings" command: every setting is persisted the moment it changes
(`zustand/persist`).

## Hilfe / Help

| Function | Implementation | Command id | Menu path | Status |
|---|---|---|---|---|
| Keyboard shortcuts | `uiStore.openShortcuts`, `ShortcutsDialog`, `commands.ts:shortcutsOf` | `help.keys` | Hilfe | 040 |
| About hortcmd | `uiStore.openAbout`, `AboutDialog` | `help.about` | Hilfe | 040 |

## Deliberately not adopted from the Total Commander template

Archive test, split/combine files, encode/decode, print, comments, save/load
selection (→ TICKET-059), checksum files, Windows shares, toolbar editor, a
"Starter" menu. None of them has an implementation in hortcmd, and this ticket
adds no functionless placeholders. The plugin toolbar stays with TICKET-103.

## Platform checks

| Platform | State |
|---|---|
| macOS | Verified in the Vite demo mode (Browser pane, `localhost:1420`) — all seven menus, both languages, dark theme, narrow window incl. the "Weitere Menüs" overflow, dialogs opened by the new entries. The reduced native menu (`src-tauri/src/menu.rs`) is untouched by this ticket. |
| Windows / Linux | Not run — no build host available in this environment. The menu is the same React component on every platform and contains no platform branch besides `view.quickLook` (macOS only) and `app.quit` (hidden on macOS), both from TICKET-040 and unchanged here. |
