# TICKET-106: Plugin management (settings)

## Goal
Users manage plugins in the settings: install, activate/deactivate,
grant permissions, uninstall, and update.

## Background
Concept.md §9 provides the "Plugins" category (Installed plugins · updates ·
preview/packer plugins). Without management, the plugin system stays unusable.

## Requirements
- Keep plugin management reachable through **Konfiguration → Einstellungen… →
  Plugins**, using the existing settings dialog and the menu architecture in
  [TICKET-040](040-native-application-menu.md). No separate Starter menu; plugin
  execution entry points remain in the plugin command bar (TICKET-103).
- Build out the **"Plugins"** category in `SettingsDialog.tsx`: list of detected plugins
  (from `list_plugins`, TICKET-100) with name, version, icon, runtime, status.
- **Install** from a local folder/archive into the app plugin folder (a marketplace
  is deliberately out of scope — see Concept.md "Later/to be clarified").
- **Activate/deactivate** per plugin; **uninstall**; **update** (version
  in the manifest).
- **Permission-grant dialog**: when activating, clearly show the permissions declared in the
  manifest and require confirmation (default-deny). Granted permissions are persisted.
- **Persistence**: activated plugins + granted permissions (analogous to `settingsStore`
  persistence or backend state).
- Security note for **native** plugins in the grant dialog (full process privileges,
  see TICKET-102).

## Acceptance criteria
- [ ] Detected plugins are listed with metadata and status.
- [ ] A plugin can be activated/deactivated; the state survives a restart.
- [ ] On activation the requested permissions are shown and must be confirmed;
      without confirmation the plugin stays inactive.
- [ ] Uninstalling removes the plugin and its contributions (the command bar clears).
- [ ] For native plugins, the grant dialog points out the full process privileges.

## Tests
- Install the demo plugin (TICKET-107), activate it (confirm permissions), the button
  appears; deactivate → button gone; a restart preserves the state.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated (Concept.md §9 Plugins)
- Rename the ticket to *.done.md
