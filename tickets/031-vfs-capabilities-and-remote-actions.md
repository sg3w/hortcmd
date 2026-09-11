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
- Command-bar buttons, context-menu items, keyboard shortcuts, and dialogs use the
  same capability selector. Avoid separate scheme checks scattered across views.
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

- [ ] A typed capability response exists for local, SFTP, FTP/FTPS, SMB, and
      WebDAV locations.
- [ ] Local features remain available on the platforms where they currently work.
- [ ] A remote pane never invokes trash, OS opening, Quick Look, local volume, Git,
      Finder-tag, or other local-only commands.
- [ ] Unsupported context-menu and command-bar actions are disabled with an
      explanation.
- [ ] Unsupported keyboard shortcuts produce the same explanation and no backend
      invocation.
- [ ] SFTP offers the SSH terminal while FTP/FTPS, SMB, and WebDAV do not.
- [ ] Copy/move capability is evaluated for both source and destination.
- [ ] Disconnecting immediately invalidates remote capabilities.
- [ ] Direct invocation of an unsupported command returns a typed unsupported
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
