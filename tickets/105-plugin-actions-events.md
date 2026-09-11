# TICKET-105: Mediated actions & events

## Goal
Plugins can — permission-checked — trigger real actions in the commander and react to
events. Everything runs through the `PluginBroker`, which reuses existing
backend functions.

## Background
After the context handoff (104), the action side is missing: a plugin should e.g. start a
copy operation or react to selection changes. The user explicitly named
the handoff/use of **copy operations** including a **limit**.

## Requirements
- **Mediated actions** (each its own permission):
  - Open path (`op:copy` independent of that; opening possibly `read:fs`)
  - **Start a copy/move operation** (`op:copy`) — uses `copy_entries`/
    `move_entries` and **respects the copy limit** (`op:limit` to read/set)
  - Read directory (`read:fs`), read preview (`read:fs`)
  - Write file (`fs:write`)
  - Show notification (`notify`)
- **Event subscriptions**: `events:selection` (selection/path change),
  `events:transfer` (progress/completion of running operations). The broker only forwards
  events to plugins with the respective permission.
- **Permission enforcement** centrally in the broker; every call without permission is rejected.
- Reuse of the `commands/fs` internals (no duplicated FS code in the plugin path).
- Applies identically to JS (101) and native (102) plugins.

## Acceptance criteria
- [ ] A plugin with `op:copy` starts a copy operation that appears in the normal
      transfer progress of the commander.
- [ ] The configured copy limit is respected for plugin-triggered operations.
- [ ] A plugin with `events:transfer` receives progress/completion events.
- [ ] A plugin with `events:selection` receives selection/path changes.
- [ ] Every action/subscription without a matching permission is rejected.

## Tests
- The demo plugin (TICKET-107) starts a copy; progress visible, limit takes effect.
- Negative tests: action/subscription without permission → defined rejection.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated
- Rename the ticket to *.done.md
