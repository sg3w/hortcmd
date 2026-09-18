# TICKET-103: Plugin command bar & colorful icons

## Goal
Plugins get their own command bar in the commander. Each activated plugin can
contribute buttons with **colorful** icons; a click opens a plugin window or
triggers a plugin command.

## Background
Explicitly requested by the user: "the plugins get their own command bar and
can have colorful icons." The existing `CommandBar.tsx` provides the pattern
(`ToolButton`, divider), but is limited to core functions.

## Requirements
- Coordinate with [TICKET-040](../040-native-application-menu.md): core commands use
  the in-window application menu; plugin commands retain this separate plugin
  bar. Do not add a Norton Commander-style Starter menu or plugin menu contribution
  API as part of either ticket without a separate feature specification.
- New **plugin command bar** as a second bar (below the main CommandBar), only
  visible when activated plugins contribute buttons.
- Buttons from `contributes.commandbar` (TICKET-100): `title`, `icon`, `action`.
- **Colorful icons**: render emoji **or** data URI/SVG from the manifest (not limited to
  the monochrome Lucide icons); visually attributable per plugin.
- Button `action`: `openWindow` (opens the plugin window via 101/102) or
  `command` (sends a command to the running plugin via the broker).
- State via `src/features/plugins/pluginsStore.ts` (activated plugins + their
  contributions); reuse the `ToolButton` interaction pattern, but color-capable.
- Location: `src/features/plugins/PluginCommandBar.tsx`, wiring in `App.tsx`.

## Acceptance criteria
- [x] With at least one activated plugin the plugin command bar appears; without
      plugins it stays hidden.
- [x] A button shows the colorful icon and the title from the manifest.
- [x] An `openWindow` button opens the plugin window; a `command` button triggers the
      plugin command.
- [x] Buttons of multiple plugins are visually distinguishable.
- [x] The layout stays intact in light/dark and at a small window width.

## Tests
- With the demo plugin (TICKET-107): button visible, icon colorful, click opens the window.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated
- Rename the ticket to *.done.md

## Implementation evidence

Implemented and verified with `npm run check`. See
[Plugin interface](../../docs/plugin-interface.md) for the contract, security
boundaries and author instructions. Native cross-platform GUI behavior requires
verification on the corresponding operating systems.
