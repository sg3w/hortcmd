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
└── transfersStore.ts        running Copy/Move/Extract/Pack operations (two bars)
i18n/dictionaries.ts         de/en + t() mechanism
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
main.rs · lib.rs             binary entry + command registration
commands/fs/
├── mod.rs                   declares the submodules
├── dir.rs                   list_dir · list_drives · home_dir (+ DirEntry/DirListing/Drive)
├── file.rs                  copy/move/delete/mkdir + transfer infra (Prog, spawn_op,
│                            collision, cancel) — pub(crate), also used by archive
├── preview.rs               read_preview (F3) · open_path (F4)
├── archive.rs               archives (ZIP/tar/tar.gz/tar.xz/7z, password): list_archive ·
│                            extract_entries · create_archive (packing → ZIP)
└── watch.rs                 set_watched (directory watching, notify)
```

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
}
interface SideState { tabs: TabState[]; activeTab: number }
interface PanesStore { active: Side; left: SideState; right: SideState; /* …actions */ }
```

**Rules:** components subscribe only to their partial state (`panelOf(s, side).cursor`);
cursor movement changes only `cursor`; folders always before files; the "system files"
filter and the `..` row are applied on rebuild.

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
| `+` / `-` / `*` | Select / deselect / invert by pattern |
| Typing (letter/digit) | Quick filter of the active list · `Esc` clears |
| `F3`…`F8` | View · Edit · Copy · Move · New Folder · Delete |
| `Alt+F5` | Pack · `Cmd+C/X/V` clipboard · `Cmd+T/W` open/close tab |

Global listener in `useKeyboard.ts`; focus in input fields takes precedence.

---

## 7. IPC contracts

| Command | Args | Returns | Status |
|---|---|---|---|
| `list_dir` | `path` | `DirListing` | ✅ |
| `list_drives` | – | `Drive[]` | ✅ |
| `home_dir` | – | `string` | ✅ |
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

**Events:** `fs-progress` (`OpProgress` – per-file + overall progress), `fs-done`
(`OpDone` – ok/errors/cancelled), `fs-collision` (`CollisionReq` – blocks until answer),
`fs-changed` (folder path – external change, triggers a debounced auto-refresh).
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
  mark symlinks ⏳ *(data available via `is_symlink`)*
- Adjust column widths by dragging ✅ *(persisted in localStorage)*
- Show milliseconds 💭 *(niche)*

### File operations
- Use trash ✅ *(default trash, Shift = permanent — a single switch)*
- Ask before overwriting ✅ · default action on conflicts ⏳
- Checksums after copy ✅ *(SHA-256 source↔target, optional via setting)* ·
  speed limit ✅ · pause/resume ✅ · copy queue ✅ ·
  parallel copies / threads / buffer size ⏳ *(buffer currently fixed at 256 KB)*

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

### Plugins
- Installed plugins ⏳ · updates ⏳ · preview plugins ⏳ · packer plugins ⏳
  *(requires the plugin system)*

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
      configurable in the settings, empty = system default) + "Open in file manager"
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
- [x] **Permissions**: properties dialog (context menu "Properties …" / Alt+Enter,
      `PropertiesDialog.tsx` ↔ backend `commands/fs/props.rs`). Display + edit
      access rights (chmod via rwx matrix, setuid/setgid/sticky, octal field;
      `set_permissions`), display and change owner/group (chown with name or
      ID; `set_owner`), display extended attributes and ACL (`xattr`/`exacl`),
      checksums MD5/SHA-1/SHA-256 on demand (`file_checksums`). Unix parts
      (owner/xattr/ACL) behind `cfg(unix)`; Windows returns empty/None values.
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
- [ ] **Network** (identical UI): **SFTP first**, then SMB, WebDAV, FTP/FTPS;
      SSH terminal
      *(broken down in ticket series `tickets/020`–`026`: 020 VFS abstraction
      (foundation) → 021 connections/credentials (OS keychain) → 022 SFTP → 023 SMB,
      024 WebDAV, 025 FTP/FTPS → 026 SSH terminal; 027 optional KeePass backend.
      "Identical UI" via provider trait + schema registry; secrets via
      `CredentialStore` in the OS keychain (KeePass optional))*
- [ ] **Plugin system**: **define the API early**, then Rust/JS plugins,
      context-menu/preview/column/archive extensions, scripting (JS)
      *(broken down in ticket series `tickets/100`–`107`: 100 API/manifest contract →
      101 JS host, 102 native Rust host → 103 plugin command bar/colored icons →
      104 context handoff → 105 mediated actions/events → 106 management →
      107 demo plugin. Broker as the sole mediation, default-deny permissions; JS
      sandboxable, native plugins trust-based)*
- [ ] **Multi-column view** (2–4 panels), multiple windows
- [ ] **Integrate Spotlight search** (macOS)

### Cross-cutting — performance
- [ ] Directory watcher (P1) · directory cache · thumbnail cache · background index
      (with search) · lazy loading *(Virtual already present)*

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
