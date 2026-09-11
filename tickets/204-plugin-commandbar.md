# TICKET-204: Plugin command bar & colorful icons

## Goal
Plugins get their own command bar in the commander. Each activated plugin can
contribute buttons with **colorful** icons; a click opens a plugin window or
triggers a plugin command.

## Background
Explicitly requested by the user: "the plugins get their own command bar and
can have colorful icons." The existing `CommandBar.tsx` provides the pattern
(`ToolButton`, divider), but is limited to core functions.

## Requirements
- Coordinate with [TICKET-040](040-native-application-menu.md): core commands use
  the in-window application menu; plugin commands retain this separate plugin
  bar. Do not add a Total Commander-style Starter menu or plugin menu contribution
  API as part of either ticket without a separate feature specification.
- New **plugin command bar** as a second bar (below the main CommandBar), only
  visible when activated plugins contribute buttons.
- Buttons from `contributes.commandbar` (TICKET-201): `title`, `icon`, `action`.
- **Colorful icons**: render emoji **or** data URI/SVG from the manifest (not limited to
  the monochrome Lucide icons); visually attributable per plugin.
- Button `action`: `openWindow` (opens the plugin window via 202/203) or
  `command` (sends a command to the running plugin via the broker).
- State via `src/features/plugins/pluginsStore.ts` (activated plugins + their
  contributions); reuse the `ToolButton` interaction pattern, but color-capable.
- Location: `src/features/plugins/PluginCommandBar.tsx`, wiring in `App.tsx`.

## Acceptance criteria
- [ ] With at least one activated plugin the plugin command bar appears; without
      plugins it stays hidden.
- [ ] A button shows the colorful icon and the title from the manifest.
- [ ] An `openWindow` button opens the plugin window; a `command` button triggers the
      plugin command.
- [ ] Buttons of multiple plugins are visually distinguishable.
- [ ] The layout stays intact in light/dark and at a small window width.

## Tests
- With the demo plugin (TICKET-208): button visible, icon colorful, click opens the window.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated
- Rename the ticket to *.done.md
