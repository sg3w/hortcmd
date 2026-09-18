# TICKET-107: Reference/demo plugin(s)

## Goal
A complete example plugin exercises the entire API for real and serves third parties as a
template. Additionally, a minimal native plugin validates the second host.

## Background

This ticket provides technical examples and host/API verification. The first
functional plugin is the storage-usage analyzer in
[TICKET-108](../108-folder-storage-usage-visualizer.md).
An API is only proven once a real plugin uses it. The demo plugin bundles
window, command-bar button with a colorful icon, context handoff, and a mediated
action into a comprehensible example.

## Requirements
- **JS reference plugin "Path tools"** (in the repo under `plugins/path-tools/`):
  - Manifest with a colorful icon, a `contributes.commandbar` button, and the permissions
    `read:context`, `op:copy`, `ui:window`, `notify`.
  - The window shows the **current path, folder path, file path, and selection** (context from 104).
  - Action "Copy path" (to the clipboard) and "Copy selection to …"
    (**mediated copy operation** from 105, respects the copy limit).
- **Minimal native Rust plugin** (e.g. `plugins/native-hello/`): loads, reports a
  command-bar button, calls an allowed broker function — validates TICKET-102.
- Both plugins are installable/activatable via the management (106).
- Short **author docs**: how to build a plugin (manifest, permissions, SDK shim/ABI).

## Acceptance criteria
- [x] "Path tools" appears in the plugin command bar with a colorful icon after activation.
- [x] Its window shows the correct context and updates it on request.
- [x] "Copy path" and the mediated copy operation work; the copy limit
      takes effect.
- [x] The minimal native plugin loads, contributes a button, and successfully calls an
      allowed broker function.
- [x] The author docs explain the manifest, permissions, and both runtimes.

## Tests
- End-to-end in Tauri dev: install/activate both plugins, check the buttons,
  run through context + copy action + native broker function.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated (author docs)
- Rename the ticket to *.done.md


## Implementation evidence

The reference packages and English author guide are in
[plugins/README.md](../../plugins/README.md). Path tools exercises context,
clipboard feedback and reviewed copy requests. Native hello is a dependency-free
Rust cdylib compiled by the supplied build script.

`npm run check` passes. The tests execute the real JS source, install/activate
both manifests, and compile/run the actual Rust example in a helper process.
On macOS, the disposable Tauri dev smoke harness also passed: actual JS window
and iframe handshake/context, native command notification, and uninstall cleanup.
The smoke harness is retained for repeatable dev checks. Interactive clipboard
policy may require the displayed system-Copy fallback, which reports refusal
rather than claiming success. No Windows/Linux-only verification was undertaken.

The existing shared clipboard helper's silent-failure bug was recorded separately
as [TICKET-301](../301-clipboard-write-failure-reporting.md).
