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
- [ ] macOS: the button opens the Finder "Get Info" panel for the exact file or
      folder, and Finder is brought to the front.
- [ ] Windows: the button opens the Explorer properties sheet with all shell
      extension tabs, and hortcmd stays responsive while it is open.
- [ ] Linux: the button opens the desktop file manager's properties dialog where
      `FileManager1` supports it; otherwise it reveals the entry, and where
      neither is available a readable error appears in the dialog.
- [ ] Works for both files and folders.
- [ ] Works for paths containing spaces, quotes and umlauts.
- [ ] The button is disabled for remote VFS paths and inside archives.
- [ ] Closing hortcmd's properties dialog does not close the native window.
- [ ] A failure never leaves the dialog in a broken or busy state.

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
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- `npm run gen:types` run if IPC types changed
- Verified manually on at least one platform, reviewed for the others
- Existing functionality unchanged
- `Concept.md` updated (roadmap entry ticked, category tree noted)
- Rename the ticket to `015-native-system-properties.done.md`
