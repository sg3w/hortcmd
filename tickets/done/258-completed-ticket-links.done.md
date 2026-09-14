# TICKET-258: Repair links after moving completed tickets

## Problem

Moving completed tickets 048, 049, 053 and 054 into `tickets/done/` leaves ten
broken Markdown links: four in the project analysis, two in follow-up tickets,
and four relative documentation links inside the moved tickets. This blocks
`npm run check`, so this supporting repair is inserted before TICKET-055.

## Scope

- Preserve the existing ticket moves and contents.
- Update incoming links to the actual `tickets/done/` locations.
- Resolve documentation links relative to the completed tickets' new directory.
- Keep the documentation checker strict; do not hide broken links with aliases.

## Acceptance criteria

- [x] All ten reported links resolve to existing files.
- [x] `npm run check` passes.
- [x] Commit the corrected moves and references on the ticket branch, then merge
      into local `main` without pushing.

## Verification

Before the fix, `npm run check:docs` reports exactly ten dead links. The existing
documentation check is the regression check for these static path corrections.

Completed: all ten links corrected; `npm run check` passes, including 480 Rust
tests and 536 frontend tests. The four existing ticket moves are retained.
