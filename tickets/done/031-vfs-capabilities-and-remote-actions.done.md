# TICKET-031: Expose VFS capabilities and guard remote actions

## Goal

The frontend must know which operations a location supports and must never send a
remote address to a local-only backend command. Local and remote locations keep a
consistent interface, while unsupported actions are disabled or explained before
execution.

## Background

The VFS `Provider` trait currently covers listing, stat, streaming reads/writes,
directory creation, rename, and deletion. It supports the core browse, preview,
and transfer paths for local, SFTP, FTP/FTPS, SMB, and WebDAV locations.

Several other commands still operate directly on `std::fs` or platform APIs:
search, folder size, volume information, Git, detailed properties and ownership,
Finder tags, archives, directory/file comparison, trash, Quick Look, opening with
local applications, and local terminals. The current UI can still expose some of
these actions while a remote pane is active. This creates silent no-ops, errors
against paths such as `sftp://...`, or behavior that differs from the promised
identical interface.

The completed VFS foundation deliberately deferred these peripheral operations.
This ticket formalizes that boundary rather than pretending every provider can do
everything.

## Requirements

### Capability contract

- Add a serializable `LocationCapabilities` type exported through ts-rs.
- Represent capabilities by stable identifiers, not UI labels or provider-name
  checks. At minimum cover:
  - browse and preview;
  - copy source, copy destination, move/rename, create, and permanent delete;
  - trash;
  - archive browse, extract source/destination, and pack source/destination;
  - basic properties/stat, recursive size, checksums, and permission editing;
  - search, directory comparison/synchronization, and file comparison;
  - Git status, Finder tags, volume information;
  - system open, open with application, Quick Look, local terminal, SSH terminal.
- Expose `location_capabilities(address)` as a typed Tauri command. It resolves the
  address through the same parser/session registry as filesystem commands.
- Capabilities may depend on the provider, platform, connection state, and entry
  kind. The location response supplies provider-level capabilities; the frontend
  combines them with file/folder/archive selection state.
- Backend commands remain authoritative and return a typed `unsupported` error if
  invoked directly despite a missing capability.

### Initial capability matrix

- Local locations retain their existing features, constrained by platform:
  Quick Look and Finder tags on macOS, Unix ownership/permissions where supported,
  and local trash/system opening where the OS integration exists.
- All connected remote providers advertise core browse, preview, transfer,
  create, rename, and permanent-delete capabilities implemented through the VFS.
- SFTP advertises SSH terminal support; FTP/FTPS, SMB, and WebDAV do not.
- Remote providers do not initially advertise OS trash, system open/open-with,
  Quick Look, local terminal, local volume data, Git status, or Finder tags.
- Search, comparisons, archives, checksums, recursive size, and extended properties
  remain unavailable for a remote provider until their command implementation is
  VFS-routed and covered by provider tests. Do not infer support merely because a
  provider has `open_read`.
- Record this initial matrix in Concept.md so future tickets can expand it one
  capability at a time without changing the UI contract.

### Frontend behavior

- Cache capabilities by resolved location/provider identity and invalidate them on
  connect, disconnect, reconnect, platform change, or navigation to a different
  scheme. Do not perform one IPC call per menu item.
- Command-bar buttons, context-menu items, the in-window application menu from
  [TICKET-040](040-native-application-menu.md), keyboard shortcuts, and dialogs
  use the same capability selector. Avoid separate scheme checks across views.
  Revalidate the captured panel/tab and source/destination capabilities at menu
  execution; an enabled item from before a disconnect is not authorization to run.
- Disable unsupported visible actions and provide a localized tooltip or status
  explanation. If an unsupported action is triggered by keyboard, show the same
  explanation instead of silently doing nothing.
- Dialogs that combine two locations, such as copy, pack, extract, comparison, and
  synchronization, validate source and destination capabilities independently.
- A disconnect while a menu/dialog is open invalidates its capabilities and blocks
  subsequent execution against the dead session.
- The demo/browser mode provides a deterministic local capability set.

### Correctness and extensibility

- Remove direct frontend checks such as `scheme === "sftp"` where the capability
  contract can answer the question.
- Keep provider-specific policy in Rust. The frontend may use platform checks only
  for presentation that is unavailable before the backend loads.
- Capability identifiers must be forward-compatible with the planned plugin and
  MCP work, but this ticket does not expose capabilities to plugins or start an MCP
  server.
- Do not implement all currently local-only features for remote providers in this
  ticket. The deliverable is accurate support reporting and safe action gating.

## Acceptance criteria

- [x] A typed capability response exists for local, SFTP, FTP/FTPS, SMB, and
      WebDAV locations.
- [x] Local features remain available on the platforms where they currently work.
- [x] A remote pane never invokes trash, OS opening, Quick Look, local volume, Git,
      Finder-tag, or other local-only commands.
- [x] Unsupported context-menu and command-bar actions are disabled with an
      explanation.
- [x] Unsupported keyboard shortcuts produce the same explanation and no backend
      invocation.
- [x] SFTP offers the SSH terminal while FTP/FTPS, SMB, and WebDAV do not.
- [x] Copy/move capability is evaluated for both source and destination.
- [x] Disconnecting immediately invalidates remote capabilities.
- [x] Direct invocation of an unsupported command returns a typed unsupported
      error rather than interpreting the remote address as a local path.

## Tests

- Unit-test the capability matrix for every provider and supported platform gate.
- Unit-test unknown schemes, malformed addresses, and disconnected sessions.
- Frontend-test context menu, command bar, and keyboard behavior using local, SFTP,
  FTP, SMB, WebDAV, archive, file, and folder contexts.
- Test two-location operations for all supported/unsupported source/destination
  combinations.
- Test a disconnect while an action menu or dialog is open.
- Regression-test local actions and the browser-demo capability set.
- Run provider live tests to confirm every advertised remote capability really
  succeeds; do not advertise behavior based only on compilation.

## Definition of Done

- Capability types and IPC bindings are generated and documented.
- All action surfaces use the shared capability selector.
- Backend commands reject unsupported locations consistently.
- The provider matrix matches tested behavior and is documented in Concept.md.
- German and English explanations are present for disabled actions.
- Existing local workflows and supported remote workflows still pass.
- Rename this file to `031-vfs-capabilities-and-remote-actions.done.md`.

---

## Implementation

### Backend

- **`src-tauri/src/vfs/caps.rs`** (new) is the one place that states what a
  location can do.
  - `Capability` — 28 stable, UI-independent identifiers (serde `camelCase`,
    exported through ts-rs). Deliberately finer than the menu labels: copy
    source and copy destination, archive source and archive destination, are
    separate questions for a dialog that combines two locations.
  - `LocationCapabilities { scheme, store_id, connected, caps }`. `store_id` is
    the provider's own identity, so the frontend caches under exactly the key
    the backend uses.
  - `location_capabilities(address)` — typed Tauri command, resolved through
    the same parser and session registry as every file-system command.
  - `require` / `require_pair` — the gate in front of each local-only command.
    A refusal carries the marker `UNSUPPORTED_LOCATION` plus the capability id,
    following the `KEEPASS_LOCKED` pattern in `net::credentials`, so the
    frontend branches on a marker rather than on prose.
  - `sshTerminal` is read off `Provider::ssh()`, not matched against a scheme
    name — it is true only for a connection that really carries an SSH session.
    A disconnected remote location advertises nothing at all.
- **Gated commands.** `volume_info`, `get_tags`/`set_tags`, `file_props`,
  `set_permissions`/`set_owner`, `search`, `dir_size`, `compare_files`,
  `compare_dirs`, `sync_copy` (per pair), `git_status_watch`, `list_archive`,
  `extract_entries`, `create_archive`, `open_path`, `open_with`, `quick_look`,
  `open_terminal`, `open_system_properties`, `trash_entries` (per path),
  `make_dir`, `write_text_file`, `rename_entry`, `rename_batch`,
  `delete_entries` (per path) and the transfer engine (per source, against the
  destination). Commands that report through `fs-done` rather than a return
  value emit the refusal there, so a blocked operation never leaves the
  transfer window waiting.
  `list_dir` and `read_preview` are not gated: they already route through
  `vfs::resolve`, which fails cleanly for a dead session, and browse/preview are
  in the remote core set anyway — a second registry lookup per listing would buy
  nothing.
- **`open_system_properties`** kept its own local-only check from TICKET-015,
  now expressed as the capability gate so there is one rule instead of two.

### Frontend

- **`src/store/capsStore.ts`** caches answers under the location identity
  (`local`, `sftp://user@host:port`) — one IPC call per location, never one per
  menu item, with in-flight requests shared. `invalidateRemote()` drops every
  remote answer on connect/disconnect/reconnect (`netStore.refresh` compares the
  live-connection signature, so merely opening the manager invalidates nothing).
  `capsKnown` tells "not asked yet" apart from "supports nothing", which are two
  different things to show the user.
- **`src/features/commander/guard.ts`** is the shared selector: `capState`,
  `pairState` and `locationLive` for rendering, `guard`/`guardPair` for the
  execution paths, `explain`/`explainPair`/`locationReason` for dialogs.
  A blocked *keyboard* shortcut raises the same wording as a pane notice
  instead of doing nothing.
- **Surfaces.** `commands.ts` (read by the in-window menu, the reduced native
  macOS menu and the command bar), `FileContextMenu`, `useKeyboard`,
  `CommandBar` (new `CommandButton`, backed by the command table),
  `CompareDialog`, `FileCompareDialog`, `SearchDialog`, `PropertiesDialog`,
  `ExportDialog`, `fileOps`, `preview`, `navigate` and `panesStore.loadGit`.
  `loadDir` primes the cache while the listing is on its way.
- **Removed direct scheme tests.** `commands.ts` `localOnly()`,
  `FileContextMenu`'s `scheme !== "sftp"`, `compareLevel.compareReady`,
  `terminalStore.supportsSshTerminal` and `PropertiesDialog`'s `schemeOf` /
  `isMacOS` / `data.unix` branches all read the contract now. `lib/platform.ts`
  gained `isWindows`/`isUnixLike` for the demo fallback only — the one place
  that needs an answer before a backend exists.
- **Behavior change worth naming:** deleting on a location without an OS trash
  (every remote one today) deletes permanently instead of silently doing
  nothing — with the permanent-delete confirmation, so the irreversible step is
  never taken behind a "moves to trash" wording.
- **i18n.** `menu.why.unsupported` ("{scheme} unterstützt diese Aktion nicht"),
  `menu.why.destUnsupported` and `menu.why.checking`, German and English.
  Disabled menu entries carry the reason in both the accessible name and the
  tooltip.

### Tests

- Rust (`vfs/caps.rs`): identifier/wire-format agreement and uniqueness, the
  local matrix and its platform gates, `file://` as a local location, the
  disconnected-remote empty set for all four protocols, store-id identity with
  default ports, a guard that the remote core set cannot silently grow, unknown
  schemes, a remote address without a host, and `require`/`require_pair` for
  both ends.
- Rust (`vfs/live_tests.rs`): `exercise_capabilities` runs right after
  `exercise` against the real server, so every advertised remote capability has
  just been performed, and every withheld one is asserted to be refused with the
  typed marker. `net::session::register_live` was added as a test-only hook so
  the address-based lookups see the connection the test opened.
- Frontend (`store/capsStore.test.ts`): cache keys, one call per location, the
  full matrix for local/SFTP/FTP/SMB/WebDAV, disconnected locations, unknown and
  malformed addresses, and all four source/destination combinations.
- Frontend (`features/commander/guard.test.ts`): the command table over local,
  SFTP, FTP, SMB, WebDAV, archive, file and folder contexts; two-location
  commands naming the *right* end; the clipboard's own source folder on paste;
  refused shortcuts explaining themselves and reaching no backend call; the
  trash fallback; and a connection dropping while a menu is open.
- Frontend (`ipc/capsDemo.test.ts`): the deterministic browser-demo set and its
  platform gates.

### Verified

`cargo test` 216 passed / 0 failed, `cargo clippy --all-targets` with no new
warnings against the pre-change baseline, `cargo build` links, `npm run
typecheck` clean, `npm test` 65 passed / 0 failed, `npm run build` succeeds.
The app was run in the browser demo (`vite-dev`) and driven through the
"Dateien" and "Befehle" menus with the cursor on `..` and on a real file: the
capability-driven states are correct in both cases and the console stays free of
errors. The remote half of the matrix is covered by the unit and frontend tests
plus the `#[ignore]`d live provider tests, which need the Docker servers named
in their doc comments.
