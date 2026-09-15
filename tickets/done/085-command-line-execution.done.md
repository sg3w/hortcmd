# TICKET-085: Implement the command-line control

## Problem

Ticket 249's shell inventory found an unnamed command-line input in App.tsx with
no state, submission handler or backend command. Text can be entered but Enter
does nothing. Until execution exists, ticket 249 names and disables the field
with an explicit unavailable explanation so it does not advertise working input.

## Requirements

- Define command execution, working-directory, cancellation and output behavior.
- Implement system/process work in Rust with typed Tauri commands and structured
  errors; keep the frontend responsible for input and presentation only.
- Respect platform shell differences and avoid surprising execution when a
  dialog or menu owns the keyboard.
- Enable the input once the complete workflow is implemented; document shortcuts.
- Add meaningful execution and keyboard tests, run `npm run check`, and commit
  the ticket locally without pushing.

## Scheduling

After the requested sequence through 065. This is an unfinished feature, separate
from the core file-operation keyboard workflows of ticket 249.

## Completion

Implemented typed Rust start/status/cancel commands, bounded stdout/stderr, a
single active process, shell-specific execution in the captured local directory,
and compact lazy-loaded frontend controls with keyboard ownership guards.
Behavior and shortcuts: `docs/command-line.md`. Native platform follow-up: 274.
Validation: Rust execution tests, frontend keyboard tests and `npm run check`.
