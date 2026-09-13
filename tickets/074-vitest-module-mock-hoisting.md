# TICKET-074: Make test transport mock initialization explicit

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

Follow-up after the requested 043–065 sequence. Current Vitest 4 executes the
mocks correctly; this does not prevent accessibility work. Found while testing
043 after dependency update 073.
