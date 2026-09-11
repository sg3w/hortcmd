# TICKET-035: Modularize high-complexity frontend and transfer subsystems

## Goal

Create maintainable, behavior-oriented module boundaries for the largest source
files without changing user-visible behavior or weakening the typed IPC and VFS
contracts.

## Background

Several files now combine unrelated responsibilities: `src-tauri/src/commands/fs/file.rs`
is about 1,329 lines, `src/components/ui/SettingsDialog.tsx` about 1,280 lines,
`src/ipc/client.ts` about 1,112 lines, `src/i18n/dictionaries.ts` about 1,028
lines, and `src/store/panesStore.ts` about 646 lines. Size alone is not a defect,
but these modules mix transport adapters, demo behavior, queueing, UI sections,
state transitions, and domain logic. That raises review cost and makes focused
testing difficult as remote providers and external interfaces expand.

The refactor must be protected by the test foundation from TICKET-033. It must
not become a broad redesign or alter public command names as a side effect.

## Requirements

- Record the current responsibilities and dependency directions of each target
  module before moving code.
- Split the Rust transfer implementation into focused units for operation state,
  planning/collisions, sequential copy, parallel copy, move semantics, progress,
  and command adapters.
- Split the frontend IPC layer into domain clients and event routing while keeping
  one stable public import surface for callers.
- Separate browser-demo implementations from production Tauri adapters so mocks
  and demo behavior cannot accidentally diverge from the typed contract.
- Extract Settings dialog categories into independently testable sections with a
  shared form/control vocabulary.
- Extract pure pane transitions, sorting, session restoration, and asynchronous
  loading coordination from the Zustand store where doing so improves tests.
- Partition translation resources by documented feature namespaces while keeping
  compile-time key checking and complete German/English parity.
- Prevent circular dependencies with an automated dependency-boundary check or a
  documented architecture rule enforced by lint/build tooling.
- Preserve generated IPC types and avoid adding pass-through abstraction layers
  that do not own behavior.

## Acceptance Criteria

- [ ] Each extracted module has one stated responsibility and a stable owner-facing
      API.
- [ ] No target file remains a catch-all for unrelated feature domains; any file
      still above the agreed review threshold has a documented reason.
- [ ] Existing Tauri command names, event names, and serialized payloads remain
      compatible unless changed by a separately approved ticket.
- [ ] Browser-demo and native adapters satisfy the same compile-time contract.
- [ ] Translation keys remain type-safe and complete for both supported languages.
- [ ] No new circular dependency is introduced.
- [ ] User-visible behavior and performance remain equivalent.

## Tests

- Add characterization tests for transfer planning, collisions, pause/cancel,
  sequential/parallel execution, and move fallback before extracting Rust code.
- Add contract tests that run the same IPC client cases against demo and mocked
  Tauri adapters.
- Component-test every extracted Settings category and its state updates.
- Unit-test extracted pane reducers for stale loads, selection preservation, sort,
  tab closure, and session restoration.
- Run generated-binding verification before and after the refactor and compare the
  public output.
- Run formatting, strict Clippy, Rust tests, frontend tests, typecheck, and build.

## Definition of Done

- The identified large modules are decomposed along documented behavior boundaries.
- Characterization tests prove that the refactor did not change behavior.
- Dependency direction is documented and automatically checked where practical.
- Public IPC and VFS contracts remain stable.
- Reviewers can change one feature without navigating unrelated implementations.
- Concept.md and the repository structure documentation reflect the new layout.
- Rename this file to `035-modularize-high-complexity-subsystems.done.md`.
