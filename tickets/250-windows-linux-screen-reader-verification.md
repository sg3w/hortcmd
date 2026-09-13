# TICKET-250: Complete native Windows/NVDA and Linux/Orca verification

## Goal

Complete the Windows and Linux manual accessibility matrix originally specified
by [TICKET-043](043-accessibility-and-localization-quality.md), using real native
hortcmd builds with NVDA and Orca in both German and English.

## Scope and scheduling

On 2026-09-13 the user explicitly requested moving these platform checks to a
follow-up ticket numbered 250 or higher. They therefore no longer block ticket
043 or the current sequential implementation through 065. They remain unverified,
not passed. macOS/VoiceOver verification remains in 043.

## Requirements

- Prepare native Windows/NVDA and Linux/Orca test environments. Record OS, desktop
  session, screen-reader version, app revision, language and input method.
- Run the complete manual smoke script in `docs/accessibility.md` in DE and EN.
- Verify pane grids, cursor versus selection, virtualized positions, tree
  navigation, tabs, sorting, menus, disabled/checked states and overflow.
- Exercise copy, move, rename and delete on disposable files; cancel prompts,
  resolve collisions, inspect nested errors and verify focus return and isolation.
- Exercise search, settings, language switching, increased font/icon scale and
  actual 200% native page zoom. Verify all critical controls stay reachable.
- Connect, disconnect, fail authentication and retry against disposable services;
  verify useful, nonduplicated screen-reader announcements.
- Start, minimize, pause/resume, cancel and complete transfers. Confirm immediate
  state announcements and progress no more often than every ten seconds.
- Read SSH output, enter a harmless command, send Escape and close with
  Ctrl+Shift+F6/Enter. Verify navigable output and returned focus.
- Check platform shortcuts, reduced motion, high contrast/forced colors and
  system light/dark changes. Inspect non-color status cues and readable focus.
- Record actual screen-reader output and observed layout; automated semantic
  tests alone do not establish a pass. File defects with reproducible steps.
- Remove disposable profiles/services/credentials and restore changed preferences.

## Acceptance criteria

- [ ] Windows/NVDA smoke matrix passes in German and English.
- [ ] Linux/Orca smoke matrix passes in German and English.
- [ ] Evidence and remaining limitations are recorded in `docs/accessibility.md`.
- [ ] Any discovered defects are resolved or explicitly scheduled with rationale.
- [ ] Temporary resources and changed system preferences are cleaned up.

## Completion

Run the repository checks for any implementation changes and create local commits
without pushing. Rename this ticket with the `.done.md` suffix only when its
manual verification requirements have actually been met.
