# TICKET-107: Reference/demo plugin(s)

## Goal
A complete example plugin exercises the entire API for real and serves third parties as a
template. Additionally, a minimal native plugin validates the second host.

## Background
An API is only proven once a real plugin uses it. The demo plugin bundles
window, command-bar button with a colorful icon, context handoff, and a mediated
action into a comprehensible example.

## Requirements
- **JS reference plugin "Path tools"** (in the repo under `plugins/pfad-werkzeuge/`):
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
- [ ] "Path tools" appears in the plugin command bar with a colorful icon after activation.
- [ ] Its window shows the correct context and updates it on request.
- [ ] "Copy path" and the mediated copy operation work; the copy limit
      takes effect.
- [ ] The minimal native plugin loads, contributes a button, and successfully calls an
      allowed broker function.
- [ ] The author docs explain the manifest, permissions, and both runtimes.

## Tests
- End-to-end in Tauri dev: install/activate both plugins, check the buttons,
  run through context + copy action + native broker function.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated (author docs)
- Rename the ticket to *.done.md
