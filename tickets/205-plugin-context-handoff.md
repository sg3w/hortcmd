# TICKET-205: Context handoff to plugins

## Goal
Plugins receive the relevant working context of the commander: general path,
folder path, file path, current selection, active side, copy limit, as well as — on
demand — a copy operation.

## Background
Explicitly requested by the user: handoff of **copy operations**,
**copy-operation limit**, **file path**, **folder path**, and **path in general**. That
is the core of what a plugin needs to do meaningful work.

## Requirements
- Pass `PluginContext` (from TICKET-201) on **window open** and additionally
  make it retrievable **on request** (broker command).
- Contents:
  - **General/active path** (`path`) and **folder path** (`folderPath`)
  - **File path** of the cursor file (`filePath`, null if no file)
  - **Selection** (`selection[]`) and **side** (`side`)
  - **Copy limit** (`copyLimit`, from `settingsStore.speedLimit`)
  - Optionally a **copy operation** (source/target) passed to the plugin
- Snapshot from `panesStore` (pattern like the dialog snapshots from TICKET-002:
  `renameSnapshot`/`exportSnapshot`).
- **Permission `read:context`** required; without the permission the broker delivers no
  context.
- Sensitive data only to plugins with the matching permission; nothing shared implicitly.

## Acceptance criteria
- [ ] A plugin with `read:context` receives a correct `PluginContext` on open
      (path, folder path, file path, selection, side, copy limit).
- [ ] The plugin can retrieve the context updated on request.
- [ ] A passed copy operation arrives fully (source/target) at the plugin.
- [ ] Without `read:context` no context is delivered.

## Tests
- The demo plugin (TICKET-208) shows the current path/file path; after navigating in the
  main window, a fresh retrieval delivers the new values.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated
- Rename the ticket to *.done.md
