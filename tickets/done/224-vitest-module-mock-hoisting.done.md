# TICKET-224: Make test transport mock initialization explicit

## Backlog placement

Moved from 074 to 224 as test harness maintenance. This is supporting
work in the 220+ backlog; the requirements and open status are unchanged.

## Problem

Vitest 4 warns that the `vi.mock` calls in `src/test/tauri.ts` are nested in
`wireModules`, although Vitest hoists them to module initialization. The `wired`
flag therefore suggests conditional behavior that does not exist. A future
Vitest release will reject this pattern.

## Scope

- Move the transport mock declarations to module scope and remove the misleading
  wiring wrapper while preserving explicit `useDemo` / `useTauri` modes.
- Preserve lazy IPC imports, per-test command/listener isolation and fail-fast
  behavior for unhandled native commands.
- Add or retain behavioral tests proving both modes and event cleanup.

## Acceptance

- No nested-module-mock warning during the frontend suite.
- Both adapter contract suites and event tests pass.
- `npm run check` passes; make one local commit, do not push.

## Priority

Follow-up after the originally requested 041–065 sequence. Current Vitest 4 executes the
mocks correctly; this does not prevent accessibility work. Found while testing
249 after dependency update 073.

## Completion
- Both Tauri module mocks are declared at module scope. Removed wireModules and
  its misleading wired flag; explicit useDemo/useTauri modes remain intact.
- Existing adapter and event suites continue to check fallback/native behavior,
  missing-command errors and listener cleanup.
- Validation: npm run check, with no nested-mock warnings.
