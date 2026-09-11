# TICKET-015: Open the native system properties from the information panel

## Goal
The properties dialog (`PropertiesDialog`, Alt+Enter) gets a button that opens
the operating system's own information/properties window for the selected file
or folder — Finder "Get Info" on macOS, the Explorer properties sheet on
Windows, the file manager's properties dialog on Linux.

## Background
The dialog already shows everything hortcmd knows about an entry: type, size
(with the async folder size from TICKET-010), timestamps, permissions,
owner/group, xattrs, ACL, Finder tags, checksums and the volume usage. What it
cannot show is the OS-specific extras the system itself surfaces — Spotlight
comments, "Open with" association, Windows security tabs and per-shell-extension
tabs, application-specific metadata pages.

Rather than reimplementing those, offer a one-click hand-off to the native
window. The backend already has the platform-conditional launcher pattern this
needs: `open_path`, `quick_look`, `open_with` and `open_terminal` in
`src-tauri/src/commands/fs/preview.rs:171-291` are all `#[cfg(target_os = …)]`
branches behind a single Tauri command.

## Requirements

### Backend
- New Tauri command `open_system_properties(path: String) -> Result<(), String>`
  in `src-tauri/src/commands/fs/preview.rs`, registered in
  `src-tauri/src/lib.rs`, following the existing `#[cfg]` branch style.
- **macOS**: open the Finder info window for the item. Practical route is
  AppleScript via `osascript`:
  `tell application "Finder" to open information window of (POSIX file "…" as alias)`,
  followed by activating Finder so the panel comes to the front. The path must be
  escaped safely — never build the script by naive string concatenation of
  user-controlled input.
- **Windows**: open the shell properties sheet via the Win32 shell API —
  `SHObjectProperties(hwnd, SHOP_FILEPATH, path, null)`, or `ShellExecuteEx`
  with the `properties` verb and `SEE_MASK_INVOKEIDLIST`. Both need COM
  initialised and a thread that keeps running while the sheet is up, so the call
  must happen on a dedicated thread and must not block the Tauri command.
  This adds a `windows` crate dependency under
  `[target.'cfg(windows)'.dependencies]`.
- **Linux**: there is no single API. Use the freedesktop D-Bus interface
  `org.freedesktop.FileManager1.ShowItemProperties(uris, startup_id)`, which
  Nautilus, Dolphin, Nemo and Thunar provide to varying degrees. If the
  interface is unavailable or the call fails, fall back to
  `ShowItems` (reveal the entry in the file manager) and, failing that, return a
  clear error — do not silently do nothing.
- The command returns promptly; the native window's lifetime is not tied to it.
- Errors come back as `Result::Err` with a usable message, no `panic!`.
- Paths with spaces, quotes, non-ASCII characters and newlines must work.

### Frontend
- A button in `PropertiesDialog` labelled e.g. "System information", with an
  icon consistent with the dialog's existing lucide icons. Place it where it does
  not compete with the destructive/edit actions already in the dialog (apply
  permissions, apply owner) — the dialog header row or a footer row.
- Wire it through `src/ipc/client.ts` in the style of the existing `openPath`.
- Errors are shown inside the dialog using the existing error banner
  (`PropertiesDialog.tsx:296`), not as a modal.
- Translations for **de** and **en** in `src/i18n/dictionaries.ts`, matching the
  existing `props.*` key naming.

### Remote paths
- The command only applies to local file-system paths. For remote VFS locations
  (`sftp://`, `smb://`, `ftp://`, `dav://` — detectable via `schemeOf` in
  `src/lib/path.ts:20`) and for entries inside an archive, the button is
  disabled with a tooltip explaining why. The backend rejects such paths as well
  rather than relying on the UI.

### Scope
- No change to what hortcmd's own dialog displays — this is a hand-off, not a
  replacement.
- No new dependency beyond the Windows shell bindings; macOS and Linux go
  through processes/D-Bus that need no extra crate. If the Linux route turns out
  to need a D-Bus crate, prefer spawning `gdbus`/`dbus-send` over pulling in a
  full binding.

## Acceptance criteria
- [x] macOS: the button opens the Finder "Get Info" panel for the exact file or
      folder, and Finder is brought to the front.
- [ ] Windows: the button opens the Explorer properties sheet with all shell
      extension tabs, and hortcmd stays responsive while it is open.
- [ ] Linux: the button opens the desktop file manager's properties dialog where
      `FileManager1` supports it; otherwise it reveals the entry, and where
      neither is available a readable error appears in the dialog.
- [x] Works for both files and folders.
- [x] Works for paths containing spaces, quotes and umlauts.
- [x] The button is disabled for remote VFS paths and inside archives.
- [x] Closing hortcmd's properties dialog does not close the native window.
- [x] A failure never leaves the dialog in a broken or busy state.

## Tests
- **macOS**: file, folder, path with spaces/umlauts, symlink, a package
  (`.app`), an entry on an external volume.
- **Windows**: file, folder, drive root, a path longer than 260 characters, a
  file with a shell extension that adds a tab (e.g. a Git-tracked file).
- **Linux**: GNOME/Nautilus, KDE/Dolphin, and a minimal session with no
  `FileManager1` provider (the error path).
- **Regression**: `open_path`, `open_with`, `quick_look` and `open_terminal`
  still behave as before; the properties dialog's existing sections are
  unchanged.

## Definition of Done
- [x] Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- `npm run gen:types` run if IPC types changed
- Verified manually on at least one platform, reviewed for the others
- Existing functionality unchanged
- `Concept.md` updated (roadmap entry ticked, category tree noted)
- Rename the ticket to `015-native-system-properties.done.md`

## Implementation

### Backend — `open_system_properties`

New command in `src-tauri/src/commands/fs/preview.rs`, registered in
`src-tauri/src/lib.rs` next to the other launchers. It first parses the argument
with `vfs::parse_address` and refuses anything whose scheme is not `Local`, then
requires `symlink_metadata` to succeed — a symlink is its own entry here, so a
broken link is still accepted. Both checks are in the backend, not only in the UI.

**macOS.** `osascript` with the script split over `-e` lines and the entry passed
as **argv**, so a name containing quotes, `&`, apostrophes, newlines or umlauts can
never change what the script does. The entry is addressed as

```applescript
open information window of (item (item 2 of argv) of folder parent_)
```

rather than the `POSIX file … as alias` route the ticket suggested. The alias
coercion *resolves* a symlink: measured against a real link, it opened the info
window of the **target** and failed outright (`-1728`) on a broken link.
Addressing the entry inside its container gives Finder the link itself
(`alias file linkdir of folder …`). A path with no container (a volume root)
falls back to the alias form. `wait_briefly` waits up to 4 s for the helper: a
non-zero exit becomes an `Err` carrying its stderr, a process still running is
treated as success and detached — the info window's lifetime is not ours.

**Windows.** `ShellExecuteExW` with the `properties` verb and
`SEE_MASK_INVOKEIDLIST | SEE_MASK_NOASYNC` on a dedicated `std::thread` that
calls `CoInitializeEx`/`CoUninitialize` around it; `NOASYNC` is what lets that
thread exit safely once the sheet is up, so no message loop is needed and the
Tauri command returns immediately. New dependency under
`[target.'cfg(windows)'.dependencies]`: `windows = "0.58"`. Its feature list
needs **`Win32_System_Registry`** on top of the obvious three — `SHELLEXECUTEINFOW`
itself is gated behind it, which a host-only `cargo check` cannot reveal.

**Linux.** `org.freedesktop.FileManager1.ShowItemProperties` over `dbus-send`,
then `gdbus`, then the same pair for `ShowItems`, then a readable error naming the
missing interface. The path becomes a `file://` URI via `file_uri`, which
percent-encodes everything outside the unreserved set — so the URI is also safe
inside the GVariant literal `gdbus` expects.

### Frontend

`openSystemProperties` in `src/ipc/client.ts` mirrors `openPath` (demo mode logs
instead of invoking). `PropertiesDialog` gained a footer row, separated from the
scrolling body by a border, with a single right-aligned **"Systeminformationen"**
button (lucide `Info`, spinner while in flight). It sits away from the
destructive/edit actions ("Anwenden", "Ändern") as the ticket asked. Failures land
in the dialog's existing error banner, and `sysBusy` is cleared in a `finally`, so
no outcome can leave the button stuck. Four new `props.system*` keys in `de`/`en`.

**No archive flag was added.** The ticket asks for the button to be disabled
inside archives, but the dialog is already unreachable there: `useKeyboard.ts:96`
requires `p.archive === null` for Alt+Enter and `FileContextMenu.tsx:287` renders
the "Eigenschaften …" item with `disabled={inArchive}`. Plumbing an `inArchive`
flag through `propsStore` was written and then removed again — it could not be
exercised, and an unreachable branch is worse than the guard that already exists.
Verified in the browser demo: inside `archive.zip`, the context-menu entry is
disabled and no dialog opens. The criterion holds structurally, one level up.

### Tests

`system_properties_tests` in `preview.rs`: remote addresses of all four schemes
are refused, a missing path is refused, and `file_uri` escaping is asserted
(Linux-only `cfg`). 112 lib tests pass.

## Verified

`npm run typecheck`, `npm run build`, `cargo check` and `cargo test --lib`
(112 passed) all pass.

**Windows branch cross-checked.** A whole-crate `cargo check
--target x86_64-pc-windows-msvc` is impossible on this machine (`aws-lc-sys`
wants the Windows SDK headers), so the branch was extracted verbatim into a
scratch crate depending only on `windows` and checked against that target. That
is what surfaced the missing `Win32_System_Registry` feature; it compiles now.
The Linux branch was likewise extracted and compiled standalone. **Neither
platform was run** — reviewed only, as the DoD allows.

**macOS, natively, through the real Tauri command.** The dev binary was wrapped
in a scratchpad `.app` (see the session memory) — with its own bundle id
`com.hortcmd.dev`, because the stale `/Applications/hortcmd.app` has been running
for days under `com.hortcmd.app` and window-level tooling cannot tell the two
apart. Alt+Enter and right-click are both unreachable from background-only
control, so the dialog was opened once by hand in the running dev app to
photograph the footer, and the command itself was then driven through `invoke`
with five paths. Results:

```
OK   …/hortcmd-015-test/Grüße "quote" & 'apo'.txt
OK   …/hortcmd-015-test/Ordner mit Leerzeichen
OK   …/hortcmd-015-test/linkdir
ERR  sftp://demo@host/tmp/x.txt   => Systeminformationen gibt es nur für lokale Dateien
ERR  …/hortcmd-015-test/does-not-exist   => Eintrag existiert nicht
```

Finder then held exactly three info windows — `Infos zu: Grüße "quote" & 'apo'.txt`,
`Infos zu: Ordner mit Leerzeichen`, `Infos zu: linkdir`. So: file and folder both
work, spaces/quotes/`&`/apostrophes/umlauts survive, and the symlink case opens the
**link's** window, not its target's. The windows stayed open after hortcmd was
restarted, which also settles "closing the dialog does not close the native window".
Separately, the bare AppleScript was measured against a package (`Calculator.app`)
and a broken symlink — both open their info window.

**Browser demo mode** (vite, `/Users/demo/Documents` ↔ an `sftp://` pane): the
footer button renders right-aligned below the divider, clicking it logs
`[demo] open_system_properties /Users/demo/Documents/config.json`, and for an entry
in the sftp pane the DOM reports `disabled: true` with
`title: "Nur für lokale Dateien verfügbar"`.

**Not proven.** The Windows and Linux paths (no such machine here) and the
external-volume case.
