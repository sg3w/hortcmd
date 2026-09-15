# TICKET-101: JS plugin host (sandboxed WebviewWindow)

## Goal
JS plugins run with their own UI in an isolated Tauri `WebviewWindow`. The
communication with the commander happens exclusively via the mediated
broker bridge from TICKET-100 — no direct IPC/file-system access.

## Background
JS plugins are the cross-platform, safely containable default case. The
window infrastructure from TICKET-002 (`src/features/dialogWindow/`,
`commands/window.rs`, `WebviewWindowBuilder`) is reused.

## Requirements
- Open the plugin window via the existing window infrastructure (its own label per
  plugin instance, title/size from the manifest) that loads the plugin `entry` file.
- **Broker bridge** (postMessage/Tauri events) between the plugin window and a
  core-side `PluginBroker`; every message is checked against the plugin's granted
  permissions before it is executed.
- **No raw IPC** in the plugin window: the plugin only gets the SDK shim
  (`src/features/plugins/pluginApi.ts`), no Tauri `invoke` capability; the
  plugin window's capabilities are correspondingly minimal.
- Lifecycle: open, focus/reuse, close; clean unregistration of the bridge.
- Location: `src-tauri/src/plugins/{broker.rs, js_host.rs}`,
  `src/features/plugins/pluginApi.ts`.

## Acceptance criteria
- [ ] A minimal "hello" JS plugin opens in its own window and renders.
- [ ] The plugin can send an allowed broker message via the SDK shim and receive a
      response.
- [ ] A message without a matching permission is rejected by the broker (error to the plugin).
- [ ] No direct Tauri `invoke` is available in the plugin window.
- [ ] The main window stays unaffected when a plugin window crashes/closes.

## Tests
- "Hello" plugin: window opens, ping/pong via the broker works.
- Negative test: message without permission → defined rejection error.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated
- Rename the ticket to *.done.md
