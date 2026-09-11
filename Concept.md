# hortcmd – Concept

A file commander in the style of Total Commander, built with **Rust + Tauri v2**
(backend) and **React + TypeScript** (frontend).

This document records the architecture, decisions, and the prioritized backlog.
It is the reference for further implementation.

---

## 1. Vision

A dual-pane file manager with keyboard-centric operation modeled on Total Commander –
but deliberately with macOS strengths (Quick Look, Finder tags, Spotlight) and developer
features (Git, terminal, SFTP) as a differentiator.

- Two equal file panes, each with its own tabs and path/directory.
- Keyboard first: `Tab` switches the pane, arrow keys move the cursor,
  `Insert` selects (`Space` = Quick Look on macOS), `Enter` opens, `Backspace`
  navigates up.
- Function-key bar F3–F8 (View, Edit, Copy, Move, New Folder, Delete).
- Column view (Name · Extension · Size · Date), sortable by column click.
- Concurrent, cancelable file operations with progress and collision dialog.

---

## 2. Technology decisions

| Area | Choice | Rationale |
|---|---|---|
| Backend | **Rust / Tauri v2** | Native file-system access, small binaries, secure IPC. |
| Frontend language | **TypeScript** | Type safety across the IPC boundary, better DX with TanStack. |
| Table | **TanStack Table** (headless) | Columns, sorting, resizing; no imposed markup. |
| Virtualization | **TanStack Virtual** | Directories with 10k+ entries stay smooth. |
| State | **Zustand** | Lean store, fine-grained selectors, transient updates. |
| Overlays/menus | **Radix UI** (shadcn style) | Context menu, dialog, select – accessible, owned in the repo. |
| Icons | **Lucide** | Lightweight, tree-shakeable; extension→icon in `lib/fileIcon.tsx`. |
| Styling | **Tailwind CSS** | Utility-first, shares CSS variables with the theme. |
| Keyboard/selection | **In-house** | No library grid reproduces the TC behavior exactly. |
| Type sync | **ts-rs** | Generates TS types from the Rust structs → guaranteed in sync. |
| Bundler | **Vite** | Fast, Tauri standard. |

**Rust crates:** `tauri`, `serde`, `sysinfo` (drives), `dirs` (home),
`ts-rs` (types), `base64` (image preview), `opener` (F4 open), `zip` (ZIP, incl.
AES password), `tar` + `flate2`/`xz2` (tar/tar.gz/tar.xz), `sevenz-rust` (7z, incl.
password).

### Deliberately NOT chosen
- **React Aria Grid** – overlaps with TanStack Table and deviates from the TC model.
- **Redux/MobX** – overhead for this scope; Zustand is enough.

---

## 3. Architecture

```
┌──────────────────────────── Frontend (WebView) ────────────────────────────┐
│  React + TS                                                                 │
│   store/        Zustand stores (panes, settings, clipboard, ops, transfers) │
│   features/     panel (TanStack Table+Virtual) · commander (keyboard/ops)   │
│   components/ui Radix dialogs (Settings, Collision, Preview, Transfer)       │
│   ipc/client.ts invoke wrapper + event bus + demo fallback                  │
└──────────────────────────────────┬──────────────────────────────────────────┘
                invoke(cmd,args)    │    Events: fs-progress · fs-done · fs-collision
┌──────────────────────────────────▼──────────────────── Backend (Rust) ──────┐
│  commands/fs/  dir · file · preview · archive                                │
│  std::fs · sysinfo · dirs · zip · base64 · opener                           │
│  Threads for transfers; pub(crate) Prog/spawn_op/OpDone shared             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Data flow (open folder):** `Enter` → `useKeyboard`/`navigate.openEntry` →
`store.loadDir/loadArchive` → `ipc.listDir` → Rust `list_dir` → `DirListing` →
store sorts/filters → only the affected panel re-renders.

**Data flow (transfer):** F5 → `fileOps` starts transfer (fire-and-forget) →
Rust thread works, sends `fs-progress`; on conflict `fs-collision` (blocks until
`resolve_collision`); completion `fs-done` → frontend refreshes both panes.

---

## 4. Folder structure

**Frontend (`src/`)**

```
main.tsx · App.tsx · index.css
ipc/
├── client.ts                invoke + event bus (onFsProgress/Done/Collision) + demo
└── bindings/                ts-rs types (DirEntry, DirListing, Drive, OpProgress,
                             OpDone, OpResult, CollisionReq, Preview)
store/
├── panesStore.ts            panes with tabs, navigation, selection, sorting, archive
├── settingsStore.ts         language/theme/system files (localStorage-persisted)
├── clipboardStore.ts        Copy/Cut → Paste (metadata)
├── opsStore.ts              confirm/input dialogs, collision queue, preview, busy
├── uiStore.ts               open state of the app-level dialogs reachable from
│                            several places (settings, shortcuts, about; TICKET-040)
├── transfersStore.ts        running Copy/Move/Extract/Pack operations (two bars)
├── noticeStore.ts           transient per-pane result messages (TICKET-059)
└── selectionFileStore.ts    open state of the save/load selection dialog (059)
i18n/dictionaries.ts         de/en + t() mechanism
features/panel/              Panel · TabBar · DriveSelect · PathBar · StatusBar
                             · FileTable (Table+Virtual) · columns
                             · FilterBar · PaneNotice (comparison filter indicator
                               + transient result message, TICKET-059)
features/commander/          useKeyboard · navigate · actions · fileOps · preview
                             · selection · markClipboard · compareLevel
                               (Mark menu, TICKET-059) · FunctionBar · CommandBar
                             · commands.ts (shared command definition + menu tree,
                               TICKET-040) · MenuBar · nativeMenu (macOS bridge)
features/net/                remote connections (021): netStore · ConnectionsDialog
                             (profiles CRUD + connect/disconnect; DriveSelect group);
                             SSH terminal (026): terminalStore · SshTerminal (xterm.js)
components/ui/               FileContextMenu · SettingsDialog · OperationDialogs
                             · PreviewDialog · TransferView
                             · ShortcutsDialog · AboutDialog (Help menu, TICKET-040)
                             · SelectionFileDialog (save/load selection, 059)
lib/                         format · fileIcon · cn · path · glob · keys · shortcut
```

**Backend (`src-tauri/src/`)**

```
main.rs · lib.rs             binary entry + command registration
menu.rs                     reduced native macOS menu (app/edit/window only;
                            TICKET-040). No "Close Window" — ⌘W belongs to the
                            panel tab; Quit closes the window so the running-
                            transfer guard applies. About/Settings/Quit emit the
                            `native-menu` event; `set_menu_language` rebuilds it.
                            No native menu on Windows/Linux.
vfs/                         VFS abstraction (identical UI) — tickets 020/022–025/031
├── mod.rs                   Provider trait · Scheme · Address parser · scheme registry
│                            (resolve: address → Arc<Provider> + provider path);
│                            local + sftp/ftp/webdav/smb, alle über die Session-Registry
├── caps.rs                  Capability/LocationCapabilities + location_capabilities:
│                            was ein Ort *jetzt* kann (Provider, Plattform, Verbindung);
│                            caps::require/require_pair ist das Tor vor jedem nur-lokalen
│                            Kommando (typisierter UNSUPPORTED_LOCATION-Fehler); TICKET-031
├── local.rs                 LocalProvider (std::fs) — local fs logic lives here
├── sftp.rs                  SftpProvider über russh/russh-sftp (async → sync via block_on)
├── ftp.rs                   FtpProvider über suppaftp (synchron); FTP + FTPS
│                            (explizit/implizit), Transfers mit eigener Verbindung
├── webdav.rs                WebdavProvider über reqwest_dav (PROPFIND/GET/PUT/
│                            MKCOL/DELETE/MOVE), Basic + Digest, TLS validiert
├── smb.rs                   SmbProvider über den reinen Rust-Client `smb` (SMB2/3,
│                            synchron); erstes Pfadsegment = Share, Wurzel listet Shares
└── live_tests.rs            Integrationstests gegen echte Server (SFTP/FTP/WebDAV/SMB
                             + SSH-Shell), `#[ignore]`; Docker-Startbefehle stehen an
                             jedem Test: `cargo test --lib live_ -- --ignored --nocapture`
net/                         Remote connection base (identical UI) — tickets 021/026/027/028
├── credentials.rs           CredentialStore trait + KeychainStore (keyring; OS keychain,
│                            no plaintext fallback) · KeepassStore (portable .kdbx,
│                            KeePassXC-compatible) · Store routes to the configured
│                            backend; net_cred_* commands (TICKET-027)
├── hostkey.rs               SSH-Host-Key-Prüfung: HostId (kanonisch host/port) ·
│                            known_hosts des Nutzers (nur lesen) + verwalteter Store
│                            in <config>/hortcmd/known_hosts (atomar, 0600) ·
│                            Decision trusted/unknown/mismatch · ConnectError
│                            (typisiert statt Fehlertext); TICKET-028
├── lifecycle.rs             Verbindungs-Zustandsmaschine (TICKET-032): Phase ·
│                            FailureKind/ConnFailure (typisiert) · Versuch mit
│                            Id + Generation (einer pro Profil) · 15 s Transport-,
│                            45 s Gesamt-Timeout · probe_transport · settle als
│                            einziges Tor in die Live-Registry
├── session.rs               Profile (persisted, no secret) · session registry
│                            (connect/disconnect/reconnect) · net_* commands
├── ssh.rs                   SshSession: authentifizierte SSH-Verbindung (russh);
│                            check_server_key prüft die Host-Identität *vor* jeder
│                            Auth-Methode; SFTP-Subsystem und Terminal teilen sie sich
└── terminal.rs              Interaktive Shell: PTY-Kanal, Ein-/Ausgabe (base64-Events),
                             Resize, Sitzungsende; ssh_* commands
commands/fs/
├── mod.rs                   declares the submodules
├── dir.rs                   list_dir (→ vfs) · list_drives · home_dir (+ DirEntry/DirListing/Drive)
├── file.rs                  copy/move/delete/mkdir/rename + transfer infra (Prog, spawn_op,
│                            collision, cancel) — all fs ops routed through the vfs registry;
│                            pub(crate), also used by archive. Ausfallsichere Übertragung
│                            (TICKET-029): Plan/staging + commit, Rollback über
│                            Backup-Rename, Reste-Meldung (`fs-leftovers`)
├── preview.rs               read_preview (F3) · open_path (F4) · open_external
│                            (validierte http(s)-Links in den Systembrowser,
│                            TICKET-030)
├── archive.rs               archives (ZIP/tar/tar.gz/tar.xz/7z, password): list_archive ·
│                            extract_entries · create_archive (packing → ZIP)
├── compare.rs               compare_dirs/sync_copy (recursive, local) + compare_levels
│                            (one visible level of two panes, provider-neutral; 059)
├── selection.rs             save_selection/load_selection: versioned JSON selection
│                            files incl. format validation and limits (TICKET-059)
└── watch.rs                 set_watched (directory watching, notify)
```

---

### WebView trust boundary (TICKET-030)

The window that renders previews is the same window that can invoke every Rust
command, so untrusted *file content* is kept away from it in four independent
layers — each one holds on its own if another is bypassed:

1. **CSP** (`tauri.conf.json`). Production: `default-src 'self'`;
   `script-src 'self'` (Tauri adds its own nonce for the IPC bootstrap);
   `style-src 'self' 'unsafe-inline'` — required by React/Radix inline styles
   and the dynamic dialog sizing, deliberately *not* widened to scripts;
   `img-src 'self' data:` — `data:` only for the base64 image previews the
   backend returns; `connect-src 'self' ipc: http://ipc.localhost` for the IPC
   transport; `object-/frame-/child-/worker-/manifest-src 'none'`,
   `base-uri 'self'`, `form-action 'none'`, `frame-ancestors 'none'`. No
   `http:`, `https:`, `*` or `unsafe-eval` anywhere. `devCsp` is a **separate**
   key covering only the Vite origin, its WebSocket and React Refresh's inline
   scripts, so production inherits none of it.
2. **Navigation policy** (`lib.rs`, `is_trusted_origin`). The main window is
   declared with `"create": false` and built in `setup`, because the policy can
   only be attached while the WebView is created. It accepts the app document
   only (`tauri:` / `http://tauri.localhost`, plus `http://localhost:1420` in
   development builds); `http(s)`, `file:`, `javascript:`, `data:`, foreign
   custom protocols and malformed URLs are refused. Blocked attempts are logged
   in development with the URL alone — never with page or file contents.
3. **Link handling.** A click inside rendered Markdown is always swallowed;
   only a validated `http(s)` URL reaches the system browser via
   `open_external`, which validates the scheme again in the backend. `open_path`
   remains the *file* entry point (F4) and is never reused as a URL escape
   hatch.
4. **Sanitization** (`lib/preview.ts`). DOMPurify runs with an explicit
   element/attribute allowlist instead of its defaults, and a URL rule that
   accepts only absolute `http(s)` and in-document fragments — a *relative*
   href is stripped too, because it is same-origin and would otherwise replace
   the running app. Anchors lose `target` and gain
   `rel="noopener noreferrer"`. Highlighted source is escaped by highlight.js
   or by hand; every `dangerouslySetInnerHTML` carries the contract it relies
   on as a comment.

Capabilities (`capabilities/default.json`) are pruned to what is actually
called; each entry names its caller. Live HTML preview stays deliberately
unimplemented.

---

### Location capabilities (TICKET-031)

Local and remote locations share one UI, but they do not share one feature set:
the VFS `Provider` trait covers browsing, preview, the streaming transfer
primitives, mkdir, rename and delete — everything else (trash, Quick Look, Git,
Finder tags, search, archives, comparisons, volume data, recursive size,
extended properties, local terminals) still runs against `std::fs` or a
platform API and only works on a **local** address.

`vfs/caps.rs` is the single place that states this. It answers, per address,
which operations that location supports right now; `location_capabilities(address)`
hands the answer to the frontend, and `caps::require` / `caps::require_pair` is
the gate every local-only command puts in front of its work. A refused command
returns an error carrying the marker `UNSUPPORTED_LOCATION` plus the capability
id, so a remote address is never read as a relative local path — and the
frontend recognizes the refusal without parsing prose (same pattern as the
KeePass `LOCKED` marker).

Two rules keep the matrix honest: a capability is advertised only where its
command is really routed through the VFS **and** covered by the live provider
tests (`vfs/live_tests.rs` asserts the matrix right after exercising the real
server), and remote capabilities are reported for a **live** session only — a
disconnected location supports nothing, which is what the UI must show the
moment a connection drops.

**Initial matrix** (identifiers are stable and forward-compatible with the
planned plugin and MCP surfaces, tickets 201–208 and 041):

| Capability | local | SFTP | FTP/FTPS | SMB | WebDAV |
|---|---|---|---|---|---|
| `browse`, `preview` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `copyFrom`, `copyTo`, `move`, `create`, `deletePermanent` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `sshTerminal` | – | ✅ | – | – | – |
| `trash` | ✅ | – | – | – | – |
| `systemOpen`, `openWith`, `localTerminal` | ✅ | – | – | – | – |
| `quickLook`, `finderTags` | macOS | – | – | – | – |
| `permissions` | Unix | – | – | – | – |
| `stat`, `recursiveSize`, `checksums` | ✅ | – | – | – | – |
| `search`, `compareDirs`, `compareFiles` | ✅ | – | – | – | – |
| `archiveBrowse`, `archiveExtractFrom/To`, `archivePackFrom/To` | ✅ | – | – | – | – |
| `gitStatus`, `volumeInfo` | ✅ | – | – | – | – |

A dash is *not* a statement that the protocol could never do it — it says the
command implementation is still local-only. Each future ticket that routes one
through the VFS moves exactly one row, without touching the UI contract.
`sshTerminal` is read off the provider (`Provider::ssh()`), not matched against
a scheme name, so it is true only for a connection that really carries an SSH
session.

**Frontend.** `store/capsStore.ts` caches the answer under the location's
identity (`local`, `sftp:user@host:port`) — one IPC call per location, never one
per menu item — and drops every remote answer on connect/disconnect/reconnect.
`features/commander/guard.ts` is the shared selector: the command table
(`commands.ts`, read by the in-window menu, the reduced native macOS menu and
the command bar), the context menu and the keyboard all ask it, so no surface
decides for itself and none can disagree with another. Unsupported actions are
disabled with a localized tooltip (`menu.why.unsupported` /
`menu.why.destUnsupported` / `menu.why.offline`); a blocked *keyboard* shortcut
raises the same wording as a pane notice instead of doing nothing. Dialogs that
combine two locations — copy, pack, extract, comparison, synchronization — ask
each side for its own half, so an allowed source is never a licence for the
target. Every execution path re-asks immediately before it acts: an item drawn
before a disconnect is not authorization to run afterwards. The browser demo
returns one deterministic local set with the same platform gates.

Direct frontend scheme tests (`scheme === "sftp"`) are gone; the platform is
only consulted where the answer is needed before the backend has spoken.

---

### Connection lifecycle (TICKET-032)

Connecting is the one operation that talks to a machine nobody here controls, so
it is the one operation with an explicit state machine: `net/lifecycle.rs`.

**One attempt per profile.** Each connect request gets an attempt id and a
monotonically increasing generation. A second click finds the running attempt
(`ConnectOutcome::Pending`) instead of opening a second socket, and
`net_cancel_connect(attempt)` stops it — idempotently, so an unknown or already
finished id is a no-op rather than an error.

**Bounded.** A **15-second transport timeout** covers DNS plus TCP: reaching the
port is the one step all four protocols share, and most of their clients cannot
express a connect timeout at all, so `probe_transport` does it uniformly and
reports `unreachable`. A **45-second overall timeout** then covers handshake,
authentication, host verification and the provider's own readiness probe. The
attempt runs on a worker thread and the command waits on a channel, so
`net_connect` never outlives the budget however the server behaves. Both are
constants, not settings.

**Only a finished attempt may win.** `lifecycle::settle` is the single place a
provider enters the live registry, and it installs only when the attempt is still
the current generation and not cancelled. Everything follows from that:

* a cancelled or superseded late success is dropped — which closes the connection
  it opened — and never replaces the authoritative session;
* a **failed reconnect leaves the previously working session connected**, because
  nothing is removed before a replacement is ready;
* `net_disconnect` cancels a pending attempt first, so an attempt in flight
  cannot install its provider behind the teardown.

**Typed outcomes.** `FailureKind` separates `timeout`, `cancelled`, `auth`,
`hostKey`, `unreachable`, `protocol` and `keychain`. Cancellation, timeout, host
key, keychain and unreachable are *structural* — they come from the phase that
produced them, never from reading a message. Only the protocol clients still hand
back prose, and `classify_protocol_error` separates a refused sign-in from the
rest with a narrow vocabulary, defaulting to `protocol` when unsure (TICKET-036
replaces this with structured errors). Errors that travel as strings carry a
`CONN_*` marker, the same pattern as `KEEPASS_LOCKED` and `UNSUPPORTED_LOCATION`.

**Connection loss.** `Provider::transport_alive` is answered from local state, so
the status refresh can ask every live provider without a round trip; SFTP reports
its russh handle. A shell whose channel ends while the transport is closed
reports the loss too. Either way `report_connection_lost` removes the session
**once**, closes the shells riding on it, and records `failed` with the reason —
nothing stays marked live after the link is gone.

**Locks.** The registry mutex is never held across a keychain prompt, a socket, or
a shell teardown: every phase runs on a worker thread and reaches into the
registry only to record a decision.

**Frontend.** `ConnectionStatus` carries `phase`, `attempt` and `error`, so the
dialog adopts authoritative state when it reopens instead of resetting itself to
"disconnected". `net-state` pushes the whole list on every transition, which is
what keeps the manager and the location pickers from disagreeing.
`features/net/lifecycleUi.ts` turns a status into what the row offers — connect,
cancel, disconnect or a spinner, plus what editing/deleting may do meanwhile — so
that decision is stated once and tested directly rather than living in JSX.

---

## 5. State model (store)

```ts
type Side = 'left' | 'right';
type SortKey = 'name' | 'ext' | 'size' | 'date';
interface Row extends DirEntry { parent?: boolean }   // parent = ".." row

interface TabState {
  path: string;
  archive: string | null;        // while ZIP-browsing: path of the .zip, otherwise null
  raw: DirEntry[];               // unfiltered from the backend
  entries: Row[];                // filtered + sorted (+ "..")
  cursor: number;
  selected: Set<string>;
  anchor: number;                // anchor row for Shift range selection
  shiftBase: Set<string> | null; // selection snapshot during a Shift session
  sort: { key: SortKey; asc: boolean };
  // TICKET-059 — both temporary, both per tab:
  snapshot: { location: string; names: string[] } | null;  // "remember selection"
  compareHidden: Set<string> | null;                       // comparison filter
}
interface SideState { tabs: TabState[]; activeTab: number }
interface PanesStore { active: Side; left: SideState; right: SideState; /* …actions */ }
```

**Rules:** components subscribe only to their partial state (`panelOf(s, side).cursor`);
cursor movement changes only `cursor`; folders always before files; the "system files"
filter and the `..` row are applied on rebuild. Anything that narrows the visible
rows also prunes `selected` to them (TICKET-059) — a hidden entry must never stay
an operation target — while `snapshot` is deliberately left alone, so a filter
does not eat a remembered selection. `snapshot` is keyed by `locationOf(tab)`
(provider identity + folder + archive inner path), never by row index.

---

## 6. Keyboard model

| Key | Action |
|---|---|
| `Tab` | Switch active pane |
| `↑`/`↓`, `PageUp/Down`, `Home/End` | Move / jump cursor |
| `Enter` | Open folder · open file with default program · enter ZIP |
| `Backspace` | Up (or leave archive) |
| `Insert` | Select + cursor down (`Space` outside macOS as well) |
| `Space` | macOS: Quick Look · otherwise: select |
| `Shift+↑/↓` | Range selection · `Cmd/Ctrl+Click` single toggle · `Shift+Click` range |
| `+` / `-` / `*` | Select / deselect by pattern · invert (files only) |
| `Num +` / `Num −` / `Num *` | Same as above from the keypad |
| `⌘/Ctrl+Num +` / `⌘/Ctrl+Num −` | Select all files / clear the selection |
| `⌥/Alt+Num +` | Select all files with the cursor file's extension |
| `Num /` | Restore the remembered selection |
| `⇧F2` | Compare the two panes and mark the differences |
| Typing (letter/digit) | Quick filter of the active list · `Esc` clears |
| `F3`…`F8` | View · Edit · Copy · Move · New Folder · Delete |
| `Alt+F5` | Pack · `Cmd+C/X/V` clipboard · `Cmd+T/W` open/close tab |
| `Alt+F7` | Search · `Alt+←/→` history · `Alt+Enter` properties |
| `Cmd/Ctrl+,` | Settings (on macOS also from the native app menu) |

Global listener in `useKeyboard.ts`; focus in input fields takes precedence, and
so does an open dialog or menu (`lib/keys.ts` marker + `role="menu"` guard), so a
keystroke never triggers both a menu action and a file action (TICKET-040).
The shortcuts shown in the menu come from the same command definition as the
Help → keyboard overview (`features/commander/commands.ts`), rendered per
platform by `lib/shortcut.ts` (⌘C on macOS, Ctrl+C elsewhere).

### Application menu (TICKET-040, completed by TICKET-058/059)

The complete menu lives **in the window**, above the toolbar, on macOS, Windows
and Linux alike (`features/commander/MenuBar.tsx`, Radix Menubar). Categories in
order: **Dateien · Markieren · Befehle · Netz · Ansicht · Konfiguration**, with
**Hilfe** pushed to the right but inside the same `Menubar.Root`, so arrow keys
walk the whole bar. Categories that no longer fit a narrow window move into a
"Weitere Menüs" category instead of being cut off.

Every entry is a command from `features/commander/commands.ts`: stable id,
translation key, shortcut, `run`, and a `state()` returning hidden / disabled
(+ reason) / checked. Commands delegate to the existing runners (`actions.ts`,
`fileOps.ts`, `selection.ts`) and store actions — the menu adds no second
implementation of any file operation. `runCommand` re-evaluates `state()` right
before executing, so a menu left open across a folder change or a dropped remote
connection cannot act on a stale context.

| Menu | Commands |
|---|---|
| Dateien | open · view (F3) · edit (F4) · open with ▸ (configured programs, **other program…**, manage) ‖ copy (F5) · move (F6) · rename (F2) · new folder (F7) · delete (F8) · delete permanently (⇧F8) ‖ clipboard copy/cut/paste (⌘C/X/V) ‖ pack (⌥F5) · extract · batch rename · compare files by content ‖ properties (⌥⏎) ‖ quit (Windows/Linux only) |
| Markieren | **(TICKET-059, complete)** select/unselect group by pattern (+/−, with "include folders") · select all files (⌘Num +) · select no files (⌘Num −) · invert (*) · same extension (⌥Num +) ‖ remember selection · restore (Num /) · save selection to file · load selection from file ‖ copy names · names with path · names with all details · path with details · export file list ‖ compare directories (⇧F2) · mark newer, hide identical · clear comparison filter |
| Befehle | search (⌥F7) · **special search ▸ (duplicates, empty folders, large files)** · compare/synchronize directories ‖ navigation ▸ (back ⌥←, forward ⌥→, parent) · favorites ▸ · recent folders ▸ ‖ terminal in the current folder · open in the system file manager ‖ transfers ▸ (running/queued, opens the existing transfer dialog) |
| Netz | manage connections · **connect to ▸ (saved, not yet connected profiles)** · active connections ▸ ‖ disconnect current connection · open SSH terminal |
| Ansicht | layout ▸ (radio) · sort by ▸ (key + ascending/descending, radio) ‖ hidden files · **folders first** · extension column · permissions column · Git status (checkboxes) ‖ reset quick filter ‖ new tab (⌘T) · close tab (⌘W) · open folder under cursor in a new tab ‖ refresh pane · Quick Look (macOS) |
| Konfiguration | settings (⌘,) · **open settings page ▸ (all seven pages of the dialog)** ‖ appearance ▸ (system/light/dark) · language ▸ (de/en) |
| Hilfe | keyboard shortcuts (generated from the same command definition) · about hortcmd |

Sorting from the menu uses the explicit adapters `setSortKey`/`setSortAsc`
(`panesStore`); the column-click `setSort` keeps its toggle behavior.

A menu entry must run the *whole* user action, not just the store write behind
it: "hidden files" and "folders first" pair their setting with
`panesStore.rebuildAll`, "Git status" with `reloadGit` — the same follow-ups the
settings page has always made, and without them the checkbox flipped while the
list stayed as it was (found and fixed in TICKET-058). The full command ↔ menu
mapping, including what stays inside a dialog on purpose and what has no
implementation yet, lives in
[`docs/menu-command-inventory.md`](docs/menu-command-inventory.md).

**Selection model (TICKET-059).** A command whose label says *files* leaves
folders untouched; only "Keine Dateien markieren" clears a tab wholesale. `..` is
never selectable, and every command works on the *visible* rows — an entry hidden
by the quick filter or the comparison filter is pruned from the selection, so it
can never remain the target of a later copy or delete. A **snapshot** ("Auswahl
merken") is bound to the canonical location (`panesStore.locationOf`: provider
identity, folder, archive inner path), never to row indices, so sorting, a
refresh or a rename cannot move a remembered mark onto a neighbour; it is per
tab, survives a filter, and is dropped on navigation. Across restarts the
**selection file** takes over: versioned UTF-8 JSON, read, written and validated
in Rust (`commands/fs/selection.rs`, 10 MiB / 100 000 entries, direct child names
only). Loading shows a preview and never navigates or connects; a file taken
elsewhere needs an explicit opt-in before it may replace a selection.

**Pane comparison (TICKET-059)** compares only the two *visible* directory levels
and only ever changes marks and visibility — the recursive synchronisation dialog
under Befehle is untouched. It runs on the listings the panes already hold, so
local folders, every remote provider and archives share one path with no second
scan, and classifies them in Rust (`compare_levels`, beside the recursive
`compare_dirs`). "Equal" means metadata equality — name, size, timestamp — not a
content comparison. Symlinks, missing timestamps and type conflicts come back as
`unknown` and stay visible; same timestamp with a different size is a difference,
not "newer". The comparison filter is temporary, shown by `PaneNotice` with a
clear action, hides only equal files, and is dropped by navigation, a tab switch,
a refresh or a disconnect — a late result is discarded instead of applied.

**macOS system menu** is reduced to **hortcmd · Bearbeiten · Fenster**
(`src-tauri/src/menu.rs`). The Edit items are what gives the WebView's text
fields their native copy/paste/undo key equivalents — that is *text* undo, not
file undo (TICKET-048). Tauri's default "Close Window" item is gone so ⌘W stays
"close tab", and Quit closes the window instead of terminating the process, so
the running-transfer guard from TICKET-004 still applies.

---

## 7. IPC contracts

| Command | Args | Returns | Status |
|---|---|---|---|
| `list_dir` | `path` | `DirListing` | ✅ |
| `list_drives` | – | `Drive[]` | ✅ |
| `home_dir` | – | `string` | ✅ |
| `dir_size` | `path, token, onProgress` | `DirSize` (+ progress) | ✅ |
| `cancel_dir_size` | `token` | – | ✅ |
| `volume_info` | `path` | `VolumeInfo \| null` | ✅ |
| `make_dir` | `path` | `string` | ✅ |
| `copy_entries` | `id, sources[], destDir` | – (events) | ✅ |
| `move_entries` | `id, sources[], destDir` | – (events) | ✅ |
| `delete_entries` | `paths[]` | `OpResult` | ✅ |
| `cancel_transfer` | `id` | – | ✅ |
| `resolve_collision` | `reqId, action, applyAll` | – | ✅ |
| `read_preview` | `path, maxBytes` | `Preview` | ✅ |
| `open_path` | `path` | – | ✅ |
| `list_archive` | `archive, inner` | `DirListing` | ✅ |
| `extract_entries` | `id, archive, base, names[], destDir` | – (events) | ✅ |
| `create_archive` | `id, sources[], destZip` | – (events) | ✅ |
| `set_watched` | `paths[]` | – (events) | ✅ |
| `compare_levels` | `left[], right[]` (`LevelEntry`) | `LevelDiff[]` (TICKET-059) | ✅ |
| `save_selection` | `path, location, entries[], overwrite` | `boolean` (false = exists) | ✅ |
| `load_selection` | `path` | `SelectionFile` (validated) | ✅ |
| `location_capabilities` | `address` | `LocationCapabilities` (TICKET-031) | ✅ |
| `net_cancel_connect` | `attempt` | – (TICKET-032, idempotent) | ✅ |
| `set_menu_language` | `lang` | – (rebuilds the native macOS menu) | ✅ |

**Events:** `fs-progress` (`OpProgress` – per-file + overall progress), `fs-done`
(`OpDone` – ok/errors/cancelled), `fs-collision` (`CollisionReq` – blocks until answer),
`fs-changed` (folder path – external change, triggers a debounced auto-refresh),
`net-state` (`ConnectionStatus[]` – the whole connection lifecycle on every
transition, TICKET-032),
`native-menu` (id of a reduced-native-menu item — `app.about`/`app.settings`/
`app.quit`, routed to the same commands as the in-window menu, TICKET-040).
Types are generated with **ts-rs** (`npm run gen:types` → `src/ipc/bindings/`); the
`client.ts` wrapper has a demo fallback for the browser without Tauri.

---

## 8. Theme & appearance

Colors as CSS variables in `index.css`; Tailwind references them via
`theme.extend.colors`. **Dark** (default), **Light**, and **System** are implemented
and switchable via `data-theme` on `<html>`; "System" follows `prefers-color-scheme`
live (matchMedia listener in `App.tsx`). **Font/icon size** (Small/Medium/Large)
are implemented as well (see Settings).

### File color registry (`lib/fileColors.ts`, TICKET-009)

Every color the file list can apply to an entry is a **slot** in a central
registry: a semantic id (`git.modified`, `symlink`, `selection`, …), a label, and
a default per theme. Slots of kind `rule` also carry a **matcher** and a
**priority**; the first matching rule (lowest value first) colors the entry.
Slots of kind `state` have no matcher — they feed the interaction colors
(`--selection`, `--cursor-active`, `--cursor-inactive-marker`), which `index.css`
therefore no longer hardcodes but references from the registry.

`applyFileColors()` publishes every slot as a CSS variable `--fc-<id>` on
`<html>` for the resolved theme (`useFileColorVars` in `App.tsx`). File list,
properties dialog, and settings preview all reference the same variable, so they
cannot drift apart, and changing a color repaints without a re-render.

The matcher input comes from the backend, not from frontend guesswork:
`DirEntry`/`FileProps` carry `hidden`, `readonly`, and `executable`
(`commands/fs/attrs.rs`, per platform), and the Git status codes come from
`commands/fs/git.rs`.

Extension points: `registerFileColor()` adds a slot at module load (plugins), and
users add their own glob rules in the settings. Both flow through the same
resolution, and the settings UI renders whatever the registry contains — a new
rule needs no UI change.

---

## 9. Settings – category tree

The settings dialog (Radix, categories on the left, form on the right, resizable 1:3–1:1)
is organized into the following categories. Legend: **✅ present · ⏳ planned · 💭 open question**.

### General
- Language ✅ · Theme Light/Dark ✅ / System ✅
- Font ⏳ · Font size ✅ · Icon size ✅ · Compact list view ⏳
- Animations on/off 💭 · Save window position ✅ · Restore last tabs ✅
- Remember pane split ✅ *(splitter draggable, persisted in localStorage)*

### Favorites
- Manage favorite folders (add/remove) ✅
- Appear in the location picker above the drives ✅

### File view
- Show hidden/system files ✅ *(today a single "dotfiles" switch; possibly split into
  "hidden" + "system")*
- Folders first ✅ *(toggleable as an option)* · show/hide extension column ✅
- Size format (Automatic ↔ Bytes) ✅ · Date format ✅ *(DD.MM.YYYY / DD.MM.YY / ISO)*
- Permissions column ✅ *(Unix `rwxr-xr-x` via `mode`; owner column ⏳)* ·
  mark symlinks ✅ *(`l` in the permissions column + the `symlink` colour rule;
  a symlink to a folder is classified as a folder, TICKET-018)*
- Adjust column widths by dragging ✅ *(persisted in localStorage)*
- Show milliseconds 💭 *(niche)*

### File operations
- Use trash ✅ *(default trash, Shift = permanent — a single switch)*
- Ask before overwriting ✅ · default action on conflicts ⏳
- Checksums after copy ✅ *(SHA-256 source↔target, optional via setting)* ·
  speed limit ✅ · pause/resume ✅ · copy queue ✅ ·
  parallel copies / threads / buffer size ⏳ *(buffer currently fixed at 256 KB)*
- Failure-safe transfers ✅ *(opt-in, off by default; staging + commit so a
  failed overwrite cannot destroy the previous destination — TICKET-029)*

### Tabs
- New tabs next to current ⏳ · save on exit ⏳ *(= session)*
- Tab history ⏳ · tab color ⏳ · lock tab ⏳

### Preview
- Image ✅ · text/hex ✅ · syntax highlighting ✅ · Markdown ✅ · CSV table ✅ · PDF ⏳
- RAW↔preview toggleable ✅ *(Markdown/CSV/code/hex)* · video thumbnail ⏳ · audio info ⏳
  · EXIF ✅ *(Rust `kamadak-exif`)* · max file size ⏳ *(currently fixed at 256 KB)*

### Search
- Quick search while typing ⏳ · default search path ⏳ · history ⏳
- Search file contents ⏳ · regular expressions ⏳ · ignored folders ⏳

### File colors
- Configure every color rule ✅ *(Git states, symlink, executable, hidden, read-only)*
- Interaction colors ✅ *(selection, cursor active/inactive)*
- Dark and light configurable independently ✅ · live preview ✅ ·
  restore defaults ✅ · persisted in localStorage ✅
- Custom rules by wildcard pattern ✅ *(name, pattern, color, order)*

### Git
- Git status ✅ · branch ✅ *(badge in the path bar)* · on/off switch ✅
- Configure colors ✅ *(see File colors)*
- Remote ⏳ · diff view ⏳

### Network
- SFTP ⏳ · FTP/FTPS ⏳ · SMB ⏳ · WebDAV ⏳ *(large effort, per protocol)*
- SSH host-key trust ✅ *(no setting — the confirmation and the explicit
  "Drop host key…" action live in the connection manager; TICKET-028)*

### Plugins
- Installed plugins ⏳ · updates ⏳ · preview plugins ⏳ · packer plugins ⏳
  *(requires the plugin system)*

### MCP interface
- Server on/off ⏳ · port ⏳ · token (keychain) ⏳ · capability tiers
  (read/write/delete) ⏳ · allowed roots ⏳ · confirmation for write/delete ⏳ ·
  max read size ⏳ · audit log ⏳ *(TICKET-041)*

### Keyboard
- Edit shortcuts ⏳ · profiles (TC mode / Finder mode) ⏳

---

## 10. Roadmap & prioritization

Prioritization logic: **P1** completes the core of a file manager. **P2** are the
early-desired developer/convenience features and macOS differentiation. **P3** is
advanced file work. **P4** are large subsystems. Cross-cutting topics and a
"Later/Discussion" list at the end.

### ✅ Done
- Tauri scaffold, dual-pane, React+TS rework, ts-rs types
- FileTable (TanStack Table+Virtual), TC keyboard, sorting
- Selection: Shift+Arrow, Cmd/Ctrl+Click, `+`/`-`/`*` patterns
- Tabs per pane (active tab = target)
- Work-direction indicator on the splitter (arrow from the active pane to the target pane;
  copy/move always run in this direction)
- Settings modal (category tree + resizable split); language, theme Light/Dark,
  system-files filter (localStorage)
- Clipboard Copy/Cut/Paste; context menu (Paste active only when clipboard is filled;
  right-click on folder → into it, on file/empty space → current folder)
- File operations in the backend: Copy/Move/Delete/Mkdir/Paste
- Concurrent transfers (threads) with two-bar window, minimize to the status bar
- Cancel transfer; collision dialog (Overwrite/Rename/Skip + "for all")
- Preview F3 (text/image/hex); F4 open with default program; Shift+F4 "Open in editor"
  (extension-based, configurable); Enter opens files
- Browse & extract archives (F5): ZIP, tar, tar.gz, tar.xz, 7z — incl.
  password-protected ZIP/7z archives; packing (Alt+F5) produces ZIP
- Rust code modular (dir/file/preview/archive/watch)
- Draggable pane splitter (persisted); directory watcher (auto-refresh)
- Favorite folders (Settings → location picker); "Open in new tab"
- View modes Detail/List/Thumbnails/Tree+List (per pane); command bar with
  special-function icons
- ~~Real native windows instead of HTML overlays (TICKET-002)~~ — superseded again by
  TICKET-012, see below.
- Force modal when closing during running transfers (TICKET-004): closing
  the main window during active copy/move/extract/pack operations is
  intercepted via `Window.onCloseRequested`; modal forces "Cancel & Close"
  / "Wait" / "Close anyway" (`opsStore.forceClose`, `ForceCloseDialog` in
  `OperationDialogs.tsx`). Browser demo mode unchanged (no-op without the Tauri window API).
  *(Still current — concerns closing the main window, independent of
  the multi-window dialog management superseded below.)*
  - Bugfix: `ForceCloseDialog` closes via `getCurrentWindow().destroy()`
    (so `onCloseRequested` does not fire again), but `capabilities/default.json`
    only allowed `core:window:allow-close`, not `allow-destroy` → "Close
    anyway"/"Cancel & Close" failed silently and the window stayed open.
    Added `core:window:allow-destroy`. At the same time reduced the `windows` list in
    the capabilities to `["main"]` (the windows `settings/rename/export/
    compare/filecompare` no longer exist since TICKET-012).
  - Hardening `onCloseRequested` (App.tsx): now blocks only on genuinely active
    operations — `!done && !cancelled && (queued || fileTotal > 0)`. A transfer
    without progress/queue (e.g. a canceled password-extract operation that never
    becomes `done`) no longer holds the red X permanently.
- Bugfix: "Quit" (Alt+F4, `actions.ts` action `quit`) still called the
  browser API `window.close()` — a leftover from before the
  Tauri migration, ineffective on the native, OS-opened main window
  (no effect, no error). Now via
  `getCurrentWindow().close()`, so the same `onCloseRequested` handler
  as the native window button applies (incl. the transfer check above).
- ~~Real modal behavior of the dialog windows via `Window::set_enabled` (TICKET-005)~~
  and ~~main-window re-enable hardening (TICKET-011)~~ — both made obsolete by
  TICKET-012: there are no separate dialog windows anymore that would need to be
  disabled/re-enabled.
- Preserve scroll position/selection after file operations (TICKET-006):
  `reloadBoth()` (after Copy/Move/Extract/Pack/Delete/Trash) as well as Rename
  and New Folder now use `panesStore.refresh(side, {reveal:true})`
  instead of `loadDir` — cursor/selection no longer jump back to 0/empty.
  Newly added entries (target of a copy/move/paste
  operation, renamed/newly created file) are automatically selected;
  the existing `scrollToIndex(…, {align:"auto"})` effect in `FileTable`
  only scrolls when necessary. Verified in browser demo mode (cursor/
  selection stay exactly preserved after copying).
- Directory comparison no longer starts recursive (TICKET-007):
  `CompareDialog.tsx` incorrectly set "Recursive" to `true` on open
  (default state and the first comparison run); the dialog now always starts
  with the option disabled, regardless of the previous state. Verified in
  browser demo mode (default off → enabling takes effect →
  close/reopen resets).
- Window management rebuilt onto React dialogs (TICKET-012): the separate
  Tauri `WebviewWindow`s (Settings/Rename/Export/Compare/
  FileCompare, TICKET-002) are fully replaced by in-app React dialogs
  — this also made TICKET-005 (macOS modality) and TICKET-011
  (main-window re-enable) obsolete. New central building block
  `components/ui/AppDialog.tsx` (Radix `Dialog.Root`, always centered, resizable
  via a drag handle, clamped to `[minimum, 90% of the main window]`,
  reacts to main-window resize); size persisted per dialog type in
  `store/dialogSizeStore.ts` (`zustand` `persist`, independent per `dialogKey`).
  - Bugfix "app stops responding / hangs after some clicking even though no
    operation is running": `FileBrowserDialog` and `PreviewDialog` used the
    pattern `if (!x) return null` followed by `<Dialog.Root open>` (hardcoded `true`).
    On close, Radix unmounted in the open state and left `pointer-events:
    none` (scroll/focus lock) on `<body>` → the whole app unclickable.
    Both dialogs now stay permanently mounted and are controlled via
    `open={!!x}` (portal content via `{x && (…)}`), so Radix sees the
    `open→closed` transition and releases the body styles again — analogous
    to the already-correct `AppDialog`-based dialogs and the
    `open={!!…}` dialogs in `OperationDialogs.tsx`.
  All 5 dialogs lost their `win` prop/payload branching and now always read live
  from their respective stores (no more difference between
  Tauri and browser demo mode). `useKeyboard.ts` received a central guard
  (`[role="dialog"]` check) so shortcuts are no longer passed through
  to the file panes while a dialog is open. Removed: `commands/window.rs`
  entirely, the `on_window_event` hook in `lib.rs`, `features/dialogWindow/`,
  and `?win=` routing in `main.tsx`. Fully verified in browser demo mode
  (all 5 dialogs individually + simultaneously, resize, persistence, Esc, focus
  restoration, keyboard isolation); no live test against a real
  Tauri app (uncritical, since no Tauri-specific logic exists anymore).
- Made the selection in the inactive panel visible (TICKET-008): the cursor row
  in the inactive pane previously had only a 1px outline without a fill and
  was therefore hard to see. New CSS variable `--cursor-inactive-marker`
  (`rgba(154, 194, 123, 0.4)`, both themes) now fills the cursor row in the
  inactive panel with a subtle green (`FileTable.tsx`'s `stateClass()`); with
  an additional selection the yellow selection color stays dominant. Verified in
  browser demo mode in dark and light theme.
- Legend of the file colors + configurable color scheme (TICKET-009): the colors
  of the file list used to be a hardcoded Git→Tailwind-class map (`lib/gitColor.ts`),
  and nothing explained them. New central registry `lib/fileColors.ts` (see §8):
  slots with semantic id, label, per-theme default, and — for rules — matcher and
  priority; published as `--fc-*` CSS variables. The properties dialog gained a
  "Color" section showing the entry's name in its actual color plus the rule that
  produced it (only the highest-priority one, message when none applies); it
  resolves through the same registry and context as the list, so both cannot
  disagree. New settings category **File colors** with a live preview, one picker
  per theme, per-rule and global reset, plus an editor for custom wildcard rules
  (matched before the built-ins, reorderable). Prerequisites in the backend:
  `DirEntry`/`FileProps` now carry `hidden`/`readonly`/`executable` from the new
  `commands/fs/attrs.rs` (per platform, so the frontend stops guessing from the
  name/mode — this also fixes hidden-file filtering on Windows), and `classify()`
  in `git.rs` now distinguishes `untracked` (`??`) from `staged` (index-only
  change) instead of lumping both into `new`. `index.css` no longer hardcodes
  `--selection`/`--cursor-active`/`--cursor-inactive-marker` but references the
  registry. Verified in browser demo mode: all 11 rules incl. priority
  (git-ignored beats executable), custom rule beats built-ins, live update of
  list/dialog/preview, dark↔light independent, persistence across reload,
  restore defaults, and invalid configuration falling back to the defaults.
- Keystrokes confirming a dialog no longer leak into the file pane (TICKET-014):
  `Enter` in the "New folder" prompt created the folder *and* opened the cursor
  entry behind it — with the cursor on `..` the panel jumped one directory up.
  Cause: `PromptDialog` handled `Enter`/`Escape` without
  `preventDefault()`/`stopPropagation()`, and `submit()` unmounts the dialog
  synchronously while the native event is still propagating; the global handler
  on `window` (`useKeyboard.ts`) then judged the event by `document.activeElement`,
  which had already fallen back to `<body>`. Fixed on both ends: the prompt input
  consumes both keys, and the global handler no longer relies on state that the
  unmount destroys — new `lib/keys.ts` marks a keystroke as belonging to a dialog
  (`markDialogKey` as `onKeyDownCapture` on every `Dialog.Content`,
  `consumeDialogKey` where a dialog acts on the key), `useKeyboard` skips marked
  events, reads `e.target` instead of `document.activeElement`, and additionally
  ignores everything while any `[role="dialog"]` is open (covers the frame in which
  a freshly opened dialog has not taken focus yet). This covers all `requestPrompt`
  consumers at once (New Folder, Rename, pattern selection, archive password); the
  other `Enter`-submitting dialog inputs (Search, file browser, tag editor,
  editor extensions) consume the key the same way. `panesStore.refresh` gained
  `names` so New Folder/Rename reveal the entry by name instead of by diffing
  against the previous listing (a watcher refresh in between would otherwise
  swallow the reveal). Verified in browser demo mode: F7 + `Enter` keeps the
  panel in its directory (cursor on `..` as well as on a file, no `open_path`),
  F2 + `Enter` renames without navigating, pattern selection marks via `Enter`,
  `Escape` in the prompt leaves an active quick filter untouched, the **Create**
  button is unchanged, search still runs on `Enter`, and `Enter` in the pane
  without a dialog still opens the cursor entry. A `keydown` probe on `window`
  confirms the confirming keystroke never reaches the global handler.

### P1 — Complete the core
- [x] **Rename (F2)** of individual entries (also in the context menu; the dialog preselects
      the base name without extension, checks name conflicts; backend `rename_entry`)
- [x] **Trash** instead of permanent delete (Shift = permanent; backend `trash` crate;
      "Use trash" switch under Settings → File operations; separate
      dialogs: gentle "Move to trash" vs. red "Delete permanently")
- [x] **Draggable pane splitter** (persisted)
- [x] **Directory watcher** (`notify`): external changes trigger a debounced,
      cursor-preserving auto-refresh
- [x] **View modes**: Detail / List / Thumbnails / Treeview with list (switchable per pane
      via the command bar; the list view also serves as the compact view).
      Treeview: tree at the drive/home root, auto-expanded to the current path,
      lazily loaded; single click selects, double click/Enter loads into the list
- [x] **Command bar** with icons for special functions (Refresh, Up, New Folder,
      View, Pack, Settings) + view switcher for the active pane
- [x] **Application menu in the window** (TICKET-040): the former clickable
      `span` placeholders in `App.tsx` became a real, keyboard-operable Radix
      menubar over a shared command definition (`features/commander/commands.ts`)
      that menu, Help → keyboard overview and the native macOS menu all read from;
      states (enabled + reason, checkbox, radio) are re-checked before execution.
      macOS keeps only a reduced native menu (app · edit · window) without the
      ⌘W "Close Window" item. Full structure and command mapping: §6
      "Application menu". Follow-up: [059](tickets/059-mark-menu-and-selection-functions.md) ✅
      owns the full Mark menu.
- [x] **Menu items completed against the implemented feature set** (TICKET-058):
      every executable user action was inventoried against the code — toolbar,
      context menu, function keys, global shortcuts, settings pages, pane/tab
      actions, connection management and transfers — and written down in
      [`docs/menu-command-inventory.md`](docs/menu-command-inventory.md) with its
      implementation proof, command id, menu path, context and availability.
      Newly wired: "open with another program" (the picker the context menu
      already used), the three remaining search modes (duplicates, empty folders,
      large files) as *Spezialsuche ▸*, connecting to a saved profile from
      *Netz › Verbinden mit ▸*, the "folders first" toggle, and direct entries to
      all seven settings pages (`uiStore` now owns the selected page, so the menu
      can jump straight to one). Corrected: the menu's "hidden files", "folders
      first" and "Git status" checkboxes now run the same follow-up as the
      settings page (`rebuildAll` / `reloadGit`) instead of flipping the switch
      while the list stayed unchanged. Deliberately *not* added: TC entries with
      no implementation behind them (archive test, split/combine, encode/decode,
      print, comments, checksum files, toolbar editor, "Starter"), and the Mark
      menu, which TICKET-059 owns in full. Verified in the browser demo mode
      (both languages, dark theme, narrow window with the overflow menu, each new
      entry executed); Windows/Linux not run — the menu is one platform-agnostic
      React component.
- [x] **"Markieren" menu complete with its selection functions** (TICKET-059):
      all sixteen functions of the model implemented, in four separator-delimited
      groups. New: select all/no files, same extension (last extension,
      case-insensitive: `a.tar.gz` → `gz`, `.env`/`README` → none), remember and
      restore a selection, save/load a selection file, the two detail clipboard
      variants (TSV with header, via the existing export formatter), and the two
      pane comparisons. File semantics throughout: folders keep their marks
      unless a pattern dialog's "include folders" says otherwise; hidden entries
      are pruned from the selection so they can never be a later operation
      target. Selection files are versioned UTF-8 JSON — read, written and
      validated in Rust (`commands/fs/selection.rs`; 10 MiB / 100 000 entries,
      direct child names only, no traversal or provider address) — deliberately
      not TC's line-based format, which cannot carry tabs or line breaks in a
      name. The comparison classifies the two already-loaded listings in Rust
      (`compare_levels`), works for local, remote and archives alike, calls only
      name+size+time equality "equal", and treats symlinks, missing timestamps
      and type conflicts as not comparable. Its filter is temporary and visible
      (`PaneNotice`), clears losslessly, and is invalidated by navigation, a tab
      switch, a refresh or a disconnect. Keypad shortcuts (Num +/−/*//, ⌘Num ±,
      ⌥Num +) were added next to the existing +/−/* and every one of them is
      also reachable from the menu. 23 new Rust tests; the frontend test runner
      remains TICKET-033's scope.
- [x] **Breadcrumb navigation** in the path bar (clickable segments jump into the
      respective folder; also works inside archives)
- [x] **Make the archive visible**: amber badge "📦 archive.zip ›" in the path bar (fixed
      left, always visible; click jumps to the archive root) + package icon and archive name
      in the tab
- [x] **Settings expansion**: categories completed (File view: folders first,
      extension column, size/date format, permissions column; theme "System";
      font/icon size). Owner column deliberately deferred.
- [x] **Column widths by dragging** adjustable in the detail view (handle at the left
      column edge; widths persisted in localStorage)
- [x] **Copy Path / Copy Filename** (context menu; "Copy path" = absolute path,
      "Copy filename" = name only; multi-selection line by line)
- [x] **Favorite folders** (Settings) — appear like "drives"
- [x] **Folder "Open in new tab"** (context menu)
- [x] **Recently visited folders / history**: Back/Forward per pane (Alt+←/→ + toolbar)
      and a global, persisted list of recently visited folders (history dropdown in the
      command bar, jump by click, "Clear history")
- [x] **Session/workspace**: the last tabs of both panes (paths, active tab, view mode)
      are saved to localStorage and restored on startup (`sessionStore` +
      `panesStore.restoreSession`); window position/size via `lib/windowState.ts`
      (`@tauri-apps/api/window`). Folders that no longer exist fall back to home.
- [x] **Symlinks to folders behave like folders** (TICKET-018): `read_dir` hands
      back `lstat` metadata, so every symlink used to arrive as a file — sorted
      among the files under "folders first", opened with the default application
      instead of entered, and showing the byte length of its target string as its
      size. `LocalProvider::list` now resolves the target once per symlink
      (`fs::metadata`, *only* for entries whose `file_type().is_symlink()` — plain
      entries cost no extra syscall: 10k of them list in the same ~23 ms as
      before) and reports `is_dir`, `size`, `modified`, `readonly` and
      `executable` from it, while `is_symlink`, the hidden flag and the mode bits
      stay link-level so the entry is still recognisable as a link. A **broken
      link keeps its place in the list** — with no target to stat it falls back to
      the link's own metadata as a 0-byte file — and opening one now reports the
      reason instead of failing silently (`open_path` propagates its error,
      `openEntry` shows it). Navigation deliberately keeps the **canonicalised**
      path, so entering a symlinked folder shows the resolved target and `..`
      leads to its real parent; that also makes a symlink cycle (`a/link → a`)
      unwalkable. The frontend needed no change: `sortEntries`, `openEntry` and
      the size column all key off `is_dir`. **SFTP** does the same via
      `SSH_FXP_STAT` (one extra round trip per symlink, none for plain entries);
      FTP has no `stat` to resolve with, and SMB/WebDAV never report symlinks —
      each noted in the code. Folder size (TICKET-010) and directory comparison
      skip symlinks by `is_symlink` *before* looking at `is_dir`, so both still
      terminate unchanged.

### P2 — Developers & convenience (early-desired) + macOS
- [~] **Git integration**: base implemented — detects Git repos (branch badge in the
      path bar) and colors entries by Git status (untracked/staged/modified/deleted/
      renamed/conflict/ignored), can be toggled off/on in the settings. Backend
      `git_status_watch` via git CLI, runs **asynchronously via `spawn_blocking`** and
      delivers the result via the event `git-support-ready` (blocks neither folder
      opening nor the UI, even for large repos; TICKET-003). Colors are configurable
      (TICKET-009).
      Open: diff view, status in the tree/tab
- [x] **Terminal in the current folder** (context menu "Open in terminal"; default terminal
      configurable in the settings, empty = system default) + "Open in file manager".
      On an **SFTP folder** the same entry opens an **integrated SSH shell** in that
      remote directory (PTY, resize, scrollback; TICKET-026) — the shell rides on the
      connection that already serves SFTP, so there is no second login
- [x] **Open file in editor** (configurable editor, alternative to F4): program assignable per
      extension (two views "By extension"/"By program") + global default
      editor; trigger selectable (Shift+F4 / context menu only / F4). Programs are chosen via
      the **reusable file-browser modal**. Backend `open_with`.
- [x] **Reusable file-browser modal** (`fileBrowserStore` + `FileBrowserDialog`,
      `selectMode` file/folder/any) — callable from anywhere in the program
- [x] **Quick Look** (Space, macOS) — native preview via `qlmanage -p` (backend
      `quick_look`); Space triggers Quick Look on macOS (selecting there via Insert),
      otherwise unchanged selection. Additionally a context-menu item "Quick Look" (macOS only).
- [x] **Quick filter** (filter while typing): typing filters the active list by
      name substring (cursor jumps to the first match); Backspace edits, Esc clears,
      Enter opens the match. Filter bar with match count; saved per tab, reset when
      navigating. `+`/`-`/`*` remain pattern selection, Space remains Quick Look.
- [x] Extend preview: **EXIF** in the image (Rust `kamadak-exif`), **Markdown** (marked +
      DOMPurify), **syntax highlighting** (highlight.js), **CSV table**; RAW↔preview
      toggleable per format, binary Hex↔Text. HTML deliberately only highlighted (no live render).
- [x] **Export file lists**: command button (icon) in the active pane opens a
      configuration dialog with live preview. Scope (whole list / only selected), format
      (text list, CSV, TSV, JSON, XML), selectable fields (full path – default, name,
      extension, size, date, type, permissions) and options (include folders, CSV/TSV header,
      size/date formatted vs. raw values). Save the result (backend `write_text_file`,
      target folder via the reusable file browser) or to the clipboard.
      When saving over an existing file, a collision prompt appears
      (confirm overwrite) via the existing confirmation dialog. Pure
      formatting in the frontend (`features/commander/exportList.ts`), writing in the backend.
- [x] **Show/edit Finder tags** (macOS): "Finder tags" section in the
      properties dialog (`PropertiesDialog.tsx`, macOS only via `isMacOS`).
      Tags are shown as colored chips (× to remove), adding via a
      name field + color choice (Finder colors 0–7). Backend `commands/fs/tags.rs`
      (`get_tags`/`set_tags`) reads/writes the xattr
      `com.apple.metadata:_kMDItemUserTags` as a **binary plist** (`plist` crate,
      restricted to macOS); an empty list removes the attribute. Non-macOS returns
      empty values or a clear error.

### P3 — Advanced file work
- [x] **Directory comparison/synchronization**: command button (icon) opens a
      dialog comparing the left against the right pane (by size +
      modification time, symlinks are skipped). Form at the top (both paths,
      re-compare, **toggleable recursive** = with/without subfolders, filter
      Differences/Only-left/Only-right/Equal with counters), list below in full
      width with status per file (equal, left/right newer, different, only
      left/right) plus sizes; window mouse-resizable. Selection via
      row checkbox + "all visible"; **copy left↔right** via
      direction-dependent buttons (source must exist), then automatically
      re-compared and the window updated. Backend `compare_dirs` + `sync_copy`
      (`commands/fs/compare.rs`) run **asynchronously via `spawn_blocking`**, so
      large trees don't block the UI; target folders are created when copying.
      The comparison **streams the results per directory** over a Tauri `Channel`
      (`on_batch`): each read level is sent to the frontend immediately and appended
      there to the list (with a run token against stale batches), so for large
      trees there is no need to wait for the overall result. The result list is
      **virtualized** (`@tanstack/react-virtual`) – only the visible rows are in the
      DOM (e.g. ~30 instead of >1000). Hard **limit of 200,000 files**: the walk then
      aborts and the backend reports the truncation; the dialog shows a
      warning ("narrow the result").
- [x] **File/binary comparison**: compares the cursor file of the left against that
      of the right pane (command-bar button `GitCompare` or context "always left
      against right"; `FileCompareDialog.tsx` ↔ backend `commands/fs/filecompare.rs`,
      `compare_files`). The backend detects text vs. binary (NUL byte/invalid UTF-8),
      returns a line-by-line diff in text mode (similar/Myers, side-by-side with
      equal/replace/delete/insert colors) and a hex comparison in binary mode
      (16 bytes/row, differing rows marked). Byte identity is checked streaming.
      Runs **asynchronously via `spawn_blocking`**; result virtualized
      (`@tanstack/react-virtual`). Hard caps (4 MB text/side, 200,000 diff
      lines, 1 MB hex/side) with a "truncated" note. (Inline word diff: open.)
- [x] **Batch rename** (pattern/regex, preview): command button (icon) in the active
      pane opens a dialog with a live preview (old → new) and conflict detection.
      Scope whole list / only selected; naming scheme with placeholders (`[N]` name,
      `[E]` extension, `[C]` counter with start/step/digits); search & replace with
      optional regex ($1 …) and case sensitivity; case conversion
      (unchanged/lower/UPPER/First upper). Conflicts (empty/invalid name, duplicate
      target names, target taken by another) are marked red and skipped. Execution in the
      backend (`rename_batch`, two-phase via temporary names → also safe for name
      swaps/chains). Name computation in the frontend (`features/commander/renameRules.ts`).
- [~] **Undo for file operations** ⏳; **copy queue** ✅; **background jobs** ✅;
      **pause/resume** ✅; **speed limit** ✅; **checksums** ✅.
      Transfer engine extended (`commands/fs/file.rs`): **pause/resume** via a
      pause flag per operation (`pause_transfer`, loop waits and reports "paused" ticks);
      **speed limit** (bytes/s, windowed throttling) and **checksum
      verification** (SHA-256 source vs. target after copying) as settings
      (`settings.speed`, `settings.verify`); **copy queue** serializes operations
      in the frontend (`fileOps.ts`: `startNextQueued`/`dequeueTransfer`, `settings.queue`),
      waiting ones show "Waiting …". Background jobs/minimize/cancel already existed.
      Transfer window: pause/resume button, "Paused"/"Waiting" status.
      **Undo deliberately deferred** (data safety: only provably reversible
      operations; its own careful step).
      **Failure-safe transfers** (TICKET-029) close the engine's remaining
      data-safety hole: a resolved overwrite used to delete the destination
      *before* copying, so any failure in between lost it. With the opt-in
      setting (`settings.safeTransfers`, **off by default** — it costs
      temporary space at the destination), each top-level source is copied
      into a hidden staging entry on the destination store and published
      with one rename. The final name appears only when the copy is
      complete and verified; an existing destination survives until the
      replacement is in place (the provider's atomic replace where it has
      one — `Provider::rename_replaces`, local only — otherwise a backup
      rename rolled back on failure); a copy-based move deletes its source
      last, and a source that will not delete is reported as "both copies
      remain" rather than repaired by removing the new one. Cancellation
      and every error path touch only the staging entry. The policy is
      snapshotted at enqueue time and passed through the IPC contract, so a
      queued transfer keeps the mode it was started with. `Provider` gained
      the capability explicitly (`supports_staging`, `rename_replaces`,
      `free_space`) instead of the engine guessing from the scheme; a
      destination that cannot commit a staged entry **fails with an
      actionable message** rather than silently writing directly. Remains
      of an interrupted run (`.hortcmd-stage-*` / `.hortcmd-backup-*`) are
      reported via `fs-leftovers` and deleted only after the user confirms
      — such an entry can hold the only copy of a file.
- [x] **More archive formats**: browse & extract for tar, tar.gz, tar.xz, and 7z
      (format detected by extension, `lib/archive.ts` ↔ `archive_kind`); password-protected
      ZIP (AES/ZipCrypto) and 7z archives: password sentinels (`PASSWORD_REQUIRED`/
      `PASSWORD_WRONG`) from the backend, the frontend asks masked, cached
      per archive (`archivePw.ts`) and reused automatically when extracting. Packing
      still produces ZIP. (Encrypted/cross-format packing: open.)
- [x] **Search**: search dialog (command-bar button + **Alt+F7**; `SearchDialog.tsx` ↔
      backend `commands/fs/search.rs`, command `search`) with four modes:
      **Files** (name as glob/regex + **file contents** as text/regex,
      case sensitivity), **duplicate finder** (size → SHA-256, groups colored),
      **empty folders**, **large files** (from a minimum size, sorted by size).
      **Ignored folders** (default: node_modules/.git/target/.cache/dist).
      Runs **asynchronously via `spawn_blocking`** and **streams matches** over a
      Tauri `Channel`; result list **virtualized** (`@tanstack/react-virtual`),
      double click reveals the match in the active pane (Reveal). Hard match limit
      (50,000) with a "truncated" note; content search up to 8 MB/file, binaries skipped.
      The tree is read by the **reusable local walker** `commands/fs/walk.rs`
      (**jwalk**, two reader threads; TICKET-013) instead of the former handwritten
      recursive collect-all function: ignored folders are pruned **before** descending,
      links are never followed (Windows junctions included), deep trees cannot overflow
      the stack, and every mode **streams** — the first match arrives after ~0.15 ms
      instead of after the whole tree (~225 ms on a 60,000-entry fixture), while a wide
      tree scans 2.1× faster than with `walkdir` (measurements:
      `docs/traversal-benchmark.md`). All limits are bounded and honestly reported:
      `search` now returns a **`SearchSummary`** whose `status` is
      `complete | truncated | limited | partial | cancelled` (plus hit/file/folder and
      skipped-entry counts) — only `complete` means the list is the full answer, and the
      dialog shows the other four. A run can be **stopped** (Stop button, `cancel_search`
      by token, also fired when the dialog closes); a closed result channel ends the scan
      as well. Duplicate mode buffers at most 300,000 candidate files, large-file mode
      keeps only the biggest 50,000 in a bounded heap instead of sorting the whole tree.
- [x] **Permissions**: properties dialog (context menu "Properties …" / Alt+Enter,
      `PropertiesDialog.tsx` ↔ backend `commands/fs/props.rs`). Display + edit
      access rights (chmod via rwx matrix, setuid/setgid/sticky, octal field;
      `set_permissions`), display and change owner/group (chown with name or
      ID; `set_owner`), display extended attributes and ACL (`xattr`/`exacl`),
      checksums MD5/SHA-1/SHA-256 on demand (`file_checksums`). Unix parts
      (owner/xattr/ACL) behind `cfg(unix)`; Windows returns empty/None values.
- [x] **Folder size & volume information** in the properties dialog (TICKET-010):
      folders show their recursive size (`commands/fs/size.rs` → `dir_size`),
      calculated on a background thread and streamed over a Tauri `Channel`, so
      the intermediate totals (bytes, files, folders) grow live next to a spinner.
      The walk is iterative (deep trees cannot overflow the stack), never follows
      symlinks (no loops), and counts unreadable entries instead of aborting
      ("… nicht lesbar (übersprungen)"). Closing the dialog cancels the task by
      token (`cancel_dir_size`); a cancel arriving before the task registers is
      kept, so it cannot be lost. In addition, every entry shows the storage of
      its volume (`commands/fs/volume.rs` → `volume_info`, via the same `sysinfo`
      abstraction as `list_drives`): the compact summary **"268 G frei von 926 G"**,
      a usage bar (red below 10 % free), mount point, file system, total, used, and
      free with percentages. The deepest matching mount point wins, compared
      component-wise ("/Volumes/Data" does not match "/Volumes/Dat"). The
      drive bar of a pane (`DriveSelect`) uses the same wording and shows
      "268 G frei von 926 G" instead of only the free space.
- [x] **Hand-off to the system's own information window** (TICKET-015): the
      properties dialog has a footer button **"Systeminformationen"** that opens
      what the OS itself shows about the entry — Spotlight comment, "Open with"
      association, Windows security and shell-extension tabs — instead of
      hortcmd reimplementing any of it. Backend `open_system_properties`
      (`commands/fs/preview.rs`, same `#[cfg]` pattern as `open_path`/`quick_look`):
      **macOS** drives Finder over `osascript`, addressing the entry as
      "item <name> of folder <parent>" so a symlink opens its own info window
      rather than its target's (and a broken link still works); every path goes
      in as an argv entry, never as script text. **Windows** calls
      `ShellExecuteExW` with the `properties` verb
      (`SEE_MASK_INVOKEIDLIST | SEE_MASK_NOASYNC`) on a dedicated COM-initialised
      thread, so the command returns at once and the app stays responsive.
      **Linux** calls `org.freedesktop.FileManager1.ShowItemProperties` over
      `dbus-send`/`gdbus` and falls back to `ShowItems`, else a readable error.
      Only local paths: a remote VFS address is refused by the backend *and*
      disables the button with a tooltip; archive members never reach the dialog
      at all. The native window outlives both the call and hortcmd's own dialog.
- [x] **Parallel copies / thread count / buffer size** (settings + backend):
      three new settings (`settings.buffer`, `settings.threads`; buffer size
      default 256 KB, thread count default **1 = sequential/unchanged**).
      Buffer size is passed through the copy path (`resolve_buf`, 4 KB–16 MB).
      **Parallel copies** via a deliberately **isolated path** (`run_parallel_copy`
      + `SharedProg` with atomic counters, `commands/fs/file.rs`), active only for
      pure **copying** and thread count > 1; collisions are resolved beforehand
      sequentially/interactively, then the file tasks are processed over N workers.
      Pause, cancel, checksums, and a **global** speed
      limit apply in parallel too. **Move stays sequential** (rename is immediate;
      copy fallback rare) and the existing single-thread path is unchanged.

### P4 — Large subsystems
- [x] **Network** (identical UI): **SFTP first**, then SMB, WebDAV, FTP/FTPS;
      SSH terminal
      *(broken down in ticket series `tickets/020`–`028` plus `031`:
      **020 VFS abstraction
      (foundation) ✅ → 021 connections/credentials (OS keychain) ✅ →
      022 SFTP ✅ → 023 SMB ✅, 024 WebDAV ✅, 025 FTP/FTPS ✅ →
      026 SSH terminal ✅ → 027 optional KeePass backend ✅ →
      028 SSH host-key verification ✅ → 031 capability contract ✅ →
      032 connection lifecycle ✅**.
      "Identical UI" via provider trait + schema registry; secrets via
      `CredentialStore` — OS keychain by default, optionally a portable
      `.kdbx` file (TICKET-027). The store is picked in the connection
      manager ("Speicher …"): file path, optional key file, and the
      clearly labeled opt-in to keep the master password in the keychain.
      Without that opt-in the master password stays in memory only and
      the file re-locks after one access ("do not remember"); a locked
      file does not fail the click — the action is parked behind the
      unlock prompt and replayed. KDBX 4 is read **and** written
      (Argon2 parameters of the file are preserved, writes go through a
      temp file + rename); legacy KDBX 3 files are read-only, with a
      message naming the upgrade)*
      **Server authentication** closes the last gap of the SSH stack
      (TICKET-028): `check_server_key` no longer accepts everything but
      matches the presented key against the user's own `known_hosts`
      (read only) and a hortcmd-managed store in the config directory.
      The check runs once while the shared SSH session is built — before
      password, key, agent or none auth — so SFTP *and* the terminal on
      it are covered by one place, and a cancelled prompt sends no
      secret at all. An unknown host returns a typed challenge
      (`ConnectOutcome`, not a parsed message) with host, port, key type
      and SHA-256 fingerprint; the one-shot trust token keeps the key in
      the backend, so only the key actually shown can be confirmed. A
      *changed* key is a hard failure showing both fingerprints with
      their source file and line, deliberately without a bypass button —
      replacing it is the separate "Drop host key…" action, which
      touches hortcmd's store only and reports conflicting entries in
      the user's files instead of editing them. A conflict in either
      source outweighs a match in the other.
      **What a location can do** is no longer guessed by the UI
      (TICKET-031): `location_capabilities(address)` answers per address,
      resolved through the same parser and session registry as the
      file-system commands, and `caps::require` refuses a local-only
      command that is handed a remote address — with a typed
      `UNSUPPORTED_LOCATION` error, never by reading `sftp://host/x` as a
      relative local path. Connected remote providers advertise the VFS
      core (browse, preview, copy, move, create, permanent delete) and,
      for SFTP alone, the SSH terminal; everything else stays local-only
      until its command implementation is VFS-routed and covered by the
      live provider tests. A disconnected location advertises nothing.
      The frontend caches the answer per connection identity and feeds
      one selector to the command table, the context menu, the toolbar,
      the native menu and the keyboard, so a disabled item, a blocked
      shortcut and a two-location dialog all explain themselves in the
      same words. The matrix is in §4 "Location capabilities".
      **Connecting itself is bounded and cancellable** (TICKET-032): one
      attempt per profile with an id and a generation, a 15-second
      transport and 45-second overall timeout, an explicit cancel, and
      `settle` as the only gate into the live registry — so a cancelled
      or superseded late result is dropped and a failed reconnect leaves
      the working session alone. Failures are typed
      (`timeout`/`cancelled`/`auth`/`hostKey`/`unreachable`/`protocol`/
      `keychain`), the manager shows a spinner with a cancel action while
      an attempt runs, and a lost connection reaches the manager and the
      location pickers as one `net-state` snapshot. Details in §4
      "Connection lifecycle".
      The **connection manager has its own toolbar button** (`CommandBar`, `Server`
      icon in its own section between navigation and file actions; TICKET-016) and
      opens globally via `useNet.getState().openManager()` — the toolbar is always
      rendered, so the manager stays reachable with zero configured connections.
      The location picker (`DriveSelect`) is therefore pure navigation again: the
      `__manage__` sentinel item is gone, and the network group — heading and
      separator included — is only rendered when at least one connection is
      actually `connected`. The in-window **Netz** menu offers the same action
      (TICKET-040).
- [ ] **Plugin system**: **define the API early**, then Rust/JS plugins,
      context-menu/preview/column/archive extensions, scripting (JS)
      *(broken down in ticket series `tickets/201`–`208`: 201 API/manifest contract →
      202 JS host, 203 native Rust host → 204 plugin command bar/colored icons →
      205 context handoff → 206 mediated actions/events → 207 management →
      208 demo plugin. Broker as the sole mediation, default-deny permissions; JS
      sandboxable, native plugins trust-based)*
- [ ] **MCP interface**: hortcmd as an **MCP server** on localhost, so an AI host
      reaches the VFS incl. the *active* remote connections and the panel context
      *(ticket `tickets/041`; bearer token in the keychain, default-deny capability
      tiers, root allowlist, confirmation for write/delete, audit log; stdio bridge
      binary for hosts that only speak stdio. Independent of the plugin series
      201–208 — there foreign code runs inside hortcmd, here hortcmd is the server.
      hortcmd as MCP *client* is deliberately not part of it)*
- [ ] **Multi-column view** (2–4 panels), multiple windows
- [ ] **Integrate Spotlight search** (macOS)

### Cross-cutting — performance
- [~] Directory watcher (P1) ✅ · **library-backed, streaming, bounded local traversal**
      (`commands/fs/walk.rs`, jwalk; TICKET-013) ✅ · directory cache ⏳ ·
      thumbnail cache ⏳ · background index (with search) ⏳ *(TICKET-054)* ·
      lazy loading *(Virtual already present)*

### 💭 Later / to be clarified
- **Read RAR** — license problem (`unrar` non-free); only via a system tool or omit
- **Three-way comparison**, **show milliseconds**, **AppleScript** — niche, low priority
- **Plugin marketplace** — only sensible after a stable plugin API
- **iCloud status / display volumes cleanly** — macOS polish, tie to Finder integration

> **Note on the long-term vision:** the differentiator arises from the
> combination *Git integration + integrated preview (image/PDF/Markdown) + terminal in the
> panel + SFTP with an identical UI + plugins*. These axes (P2→P4) should guide the
> architecture decisions – in particular, keep the **plugin API early** in mind.

---

## 11. Origin of the backlog items (tracking)

During prioritization, the earlier, unsorted roadmap was filed into §10. The
following table shows "old → new" so it stays traceable where each item
ended up (nothing was lost).

| Earlier roadmap item | Now in |
|---|---|
| Rename (F2) | **P1** – "Rename (F2)" |
| Pane split draggable (currently fixed) | **P1** – "Draggable pane splitter" |
| Watch & display directories | **P1** – "Directory watcher" (+ cross-cutting Performance) |
| Command bar with icons for special functions | **P1** – "Command bar with icons" |
| View modes Detail/List/Thumbnail (icon) | **P1** – "View modes" |
| Visualize that you're inside an archive | **P1** – "Make the archive visible (badge)" |
| Expand configuration/settings | **P1** – "Settings expansion" (+ §9 category tree) |
| Favorite folders, shown as a drive | **P1** – "Favorite folders" |
| History | **P1** – "Recently visited folders / history" |
| GIT in the folder colored, configurable | **P2** – "Git integration" (+ §9 Git category) |
| Image preview shows EXIF | **P2** – "Extend preview: EXIF …" |
| Export file lists as text/CSV | **P2** – "Export file lists" |
| Compare/synchronize directories | **P3** – "Directory comparison/synchronization" |
| Batch rename | **P3** – "Batch rename" |
| ACL/permissions/owner | **P3** – "Permissions: ACL …" ✅ |
| Undo/queue/background jobs/speed/pause | **P3** – "Undo … copy queue … pause/resume …" |
| More archive formats (tar/7z) | **P3** – "More archive formats" ✅ (read/extract + password) |

**Further filings without their own P item:**
- *"Tab folders"* → §9 › **Tabs** (tab history, lock tab, tab color).
- *"Ask before overwriting"* → already done (collision dialog), in the ✅ block or §9 with ✅.

**Deliberate deprioritization / clarification** (see §10 "Later / to be clarified"):
Read RAR (license), show milliseconds, three-way comparison, AppleScript,
plugin marketplace.
