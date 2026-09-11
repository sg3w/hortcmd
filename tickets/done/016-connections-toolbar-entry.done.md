# TICKET-016: Connection manager into its own window, reachable from the toolbar

## Goal
Managing remote connections gets its own entry point in the toolbar instead of
being hidden as a sentinel row inside the location picker. The picker's network
group then only lists connections that are actually connected — it becomes a
pure navigation target, not a place where settings are edited.

## Background
The connection manager (`ConnectionsDialog`, ticket 021) is already a
self-contained dialog mounted at `src/App.tsx:339` and opened via
`useNet.openManager()`. Today the only way to reach it is the `__manage__`
sentinel `Select.Item` in `DriveSelect`
(`src/features/panel/DriveSelect.tsx:63-67,134-146`): choosing it aborts the
navigation and opens the dialog instead.

That mixes two concerns in one control. The location picker answers "where do I
navigate to"; the manager answers "which connections exist". Consequences today:

- The network group is always rendered, even with no connection at all, because
  the manage row keeps it non-empty — so the picker shows a group heading and a
  settings row where the user expects locations.
- Both panels have their own picker, so the manage entry appears twice.
- A `Select` whose value can mean "do not change the value" is a workaround, not
  a control.

The toolbar (`CommandBar.tsx`) is the natural home: it already groups global
actions behind icon buttons with dividers. The `menu.net` menu entry
(`App.tsx:55`) currently carries no action and is a second obvious candidate.

**Note on "separate window":** TICKET-012 deliberately replaced all separate OS
windows with in-app React dialogs. This ticket therefore means *its own dialog,
reached on its own* — the existing `ConnectionsDialog` via `AppDialog` — not a
new native window. If a real OS window is intended instead, that contradicts
TICKET-012 and needs to be decided before implementation.

## Requirements

### Toolbar
- A connection button in `CommandBar.tsx`, opening the connection manager via
  `useNet.getState().openManager()`.
- Icon consistent with the existing lucide set — `Server` and `Plug`/`PlugZap`
  are already used by `ConnectionsDialog`; reuse one of those so the toolbar and
  the dialog read as the same feature.
- Placed in its own toolbar section, separated by a `Divider` from the
  navigation and the file-action groups.
- Tooltip from the existing `net.*` translations; add a `tb.net` key for **de**
  and **en** in `src/i18n/dictionaries.ts` if no fitting key exists.
- The full in-window **Netz → Verbindungen verwalten…** entry is mandatory and
  owned by [TICKET-040](040-native-application-menu.md). Reuse the same
  `useNet.openManager()` action; do not make the top-level Netz label a direct
  action, since it will open a dropdown. This ticket owns the toolbar entry.

### Location picker
- Remove the `__manage__` item and its sentinel handling from
  `DriveSelect.tsx` — `onValueChange` no longer needs a branch that suppresses
  navigation.
- The network group only lists connections with `connected === true` (the
  existing `liveConns` filter). Confirm this is what the user actually sees: the
  filter is already in place, so the perceived problem is the always-present
  manage row keeping the group alive.
- With no live connection, the whole network group — heading and separator
  included — is not rendered at all. No empty group, no dangling separator.
- Favorites, drives and their separators keep their current behaviour and order.

### Discoverability
- Removing the manage row must not make the manager unreachable in a state where
  the toolbar is not visible. Verify whether the toolbar can be hidden by any
  setting; if it can, the manager needs a second entry point (menu entry, or the
  toolbar cannot be hidden entirely).
- A user with zero configured connections must still find the manager — this is
  the main reason the entry point moves out of a group that is now empty for
  them.

### Behaviour
- Opening the manager from the toolbar acts globally, not per panel. Connecting
  from within the dialog keeps its current behaviour of navigating the active
  panel to the new address.
- No change to `netStore` semantics, the profile format, or the backend.

## Acceptance criteria
- [x] The toolbar shows a connection button that opens the connection manager.
- [x] The location picker no longer contains a "Manage connections…" row in
      either panel.
- [x] With no connected connection, the picker shows no network group at all —
      no heading, no separator, no empty space.
- [~] With one or more connected connections, they appear in the picker and
      navigate correctly when chosen.
- [~] Disconnecting a connection in the manager removes it from both pickers.
- [~] Connecting from the manager navigates the active panel as before.
- [x] The manager is reachable with zero configured connections.
- [ ] Integration with TICKET-040 provides the same manager through the in-window
      Netz menu; this remains reachable independently of toolbar visibility.
- [x] Favorites and drives in the picker are unchanged.

`[~]` = code path unchanged and reviewed, but not exercised — needs a live
server. The TICKET-040 item stays open by design; it is that ticket's scope.

## Tests
- Manual: zero connections, one connected, one configured-but-disconnected, and
  a mix — checked in both panels.
- Connect / disconnect while a picker drop-down is open.
- Regression: navigating to a remote location still works, as does the SSH
  terminal (ticket 026) reached from a connection.
- Regression: the toolbar's existing buttons and the view switcher are unchanged
  in position and behaviour.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Existing functionality unchanged
- `Concept.md` updated (roadmap entry ticked, category tree noted)
- Rename the ticket to `016-connections-toolbar-entry.done.md`

## Implementation

- `src/features/commander/CommandBar.tsx`: new `ToolButton` with the lucide
  `Server` icon — the same icon `ConnectionsDialog` and the picker's connection
  rows already use — placed in its own section between the navigation group
  (back/forward/up/refresh/recent) and the file-action group, with a `Divider`
  on each side. `onClick` calls `useNet.getState().openManager()` (imperative,
  so the toolbar does not re-render on every connection-status change).
- Tooltip/`aria-label` reuse the existing `net.manage` key
  ("Verbindungen verwalten…" / "Manage connections…"). No `tb.net` key was
  added: the ticket asked for one only if no fitting key existed, and
  `net.manage` is exactly that string — a duplicate would have to be kept in
  sync with the TICKET-040 menu entry that will use the same wording.
- `src/features/panel/DriveSelect.tsx`: the `__manage__` `Select.Item`, the
  sentinel branch in `onValueChange`, the now-unused `openManager` binding and
  the `Settings` icon import are removed. The network `Select.Group` is wrapped
  in `liveConns.length > 0 &&`, so heading, items and separator disappear
  together when nothing is connected. Favorites, drives and their separators are
  untouched.

### Discoverability check
`CommandBar` is rendered unconditionally at `src/App.tsx:280`; no setting in
`settingsStore` hides it (only `showExtColumn` / `showPermissions` exist as
visibility flags). The toolbar entry is therefore always available, and no
second entry point is required by this ticket — the Netz menu entry stays with
TICKET-040.

## Verified

`npm run typecheck`, `npm run build` and `cargo check` all pass.

Checked in the Browser pane (vite demo mode, zero connections): the location
picker now lists only `/` and `/Users/demo` — no network heading, no manage row,
no dangling separator — and the new toolbar button opens the connection manager,
which shows "Noch keine Verbindungen." plus "Neue Verbindung". That covers the
zero-connection and zero-configured-connection criteria in both panels (both
pickers render the same component).

Not verified here: the connected / mixed states, disconnect-removes-from-picker,
connect-navigates-active-panel and the SSH-terminal regression — demo mode has no
backend, so no connection can be established. The rendering of connected entries
itself is unchanged code, only wrapped in the new conditional.
