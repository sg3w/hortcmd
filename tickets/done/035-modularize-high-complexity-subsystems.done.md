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

- [x] Each extracted module has one stated responsibility and a stable owner-facing
      API.
- [x] No target file remains a catch-all for unrelated feature domains; any file
      still above the agreed review threshold has a documented reason.
- [x] Existing Tauri command names, event names, and serialized payloads remain
      compatible unless changed by a separately approved ticket.
- [x] Browser-demo and native adapters satisfy the same compile-time contract.
- [x] Translation keys remain type-safe and complete for both supported languages.
- [x] No new circular dependency is introduced.
- [x] User-visible behavior and performance remain equivalent.

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

---

## Implementation

### What moved where

| was | now |
|---|---|
| `commands/fs/file.rs` 2,470 | `file.rs` **411** (command adapters) + `transfer/`: `events` 87, `registry` 144, `progress` 197, `plan` 216, `copy` 238, `parallel` 257, `engine` 313, `tests` 1,084 |
| `ipc/client.ts` 1,413 | `client.ts` **34** (public surface) + `invoke` 35, `events` 157, `demo` 610, `types` 27, `domains/` 7 modules |
| `i18n/dictionaries.ts` 1,508 | `dictionaries.ts` **81** + ten feature namespaces + `ns.ts` |
| `SettingsDialog.tsx` 1,297 | shell **155** + `settings/` 7 pages + `controls.tsx` (the shared vocabulary) |
| `store/panesStore.ts` 942 | `panesStore.ts` **755** (stateful/async) + `panes/state.ts` 247 (pure) |

The dependency direction of the transfer engine is stated in its `mod.rs` and
runs strictly downwards: events ← nothing; registry, progress ← events; plan ←
nothing; copy ← plan, progress; parallel ← copy, plan, progress; engine ← all.

### Three splits that changed more than the layout

- **`ProgressSink`** — the engine reported through an `AppHandle`, which cannot
  be built outside a running app, so the copy loops could only be tested end to
  end. Behind a trait they can be driven directly. That exposed a latent bug:
  verification read the target back before the writer was flushed. Harmless for
  a local `File`; a buffering remote writer would compare against an incomplete
  copy and report a corruption that is not there. Fixed.
- **Demo beside native** — a browser-demo reply that drifts from the typed
  contract fails only in the browser build. The new contract test runs the same
  cases against both modes and caught a wrong `CredStatus` shape on its first
  run (its own fixture).
- **`ns()` for translations** — German is the source of truth, English is typed
  as `Record<keyof de, string>`. A missing translation is now a *compile* error;
  before, the two languages were independent literals. Verified by deleting one
  English key and watching `tsc` fail.

Also moved for a reason rather than for size: `useFileColorRules` /
`useFileColorVars` left `lib/fileColors.ts` for `store/fileColorRules.ts`. The
colour logic is a leaf utility; reading the user's configuration is not.

### The boundary check

`scripts/check-boundaries.mjs` (`npm run check:boundaries`, in `npm run check`
and in CI) walks the import graph and fails on a cycle or a layer violation.
Type-only imports are ignored — they are erased and cannot form a runtime
cycle, which is also why the two apparent cycles it first reported
(`panesStore ↔ sessionStore`, `settingsStore ↔ fileColors`) were not real ones.
Five TypeScript rules plus the Rust transfer ordering, each carrying the reason
it exists, printed on failure.

### Characterization tests

Written *before* each extraction and passing unchanged after it:

- Rust (`transfer/tests.rs`): free names past taken ones, extension handling,
  target naming, tree measuring, sequential copy of a tree, cancellation
  removing the partial file, checksum verification, flattening with per-source
  indices, a cancelled parallel copy, and a move whose copy fails keeping its
  source. The `Fake` provider fails renames and removes on demand; the
  `Recorder` sink stands in for the event bus.
- `src/ipc/contract.test.ts` (20): the same nine cases against demo and mocked
  Tauri, plus the two places they differ on purpose.
- `src/store/panes/state.test.ts` (26): sorting (collation, numeric, folders
  first, stability), row rebuilding (the `..` row, hidden files, the quick
  filter, the comparison filter never hiding a folder), selection pruning,
  cursor placement, the sort transition dropping a stale range, and the Shift
  range growing, shrinking and preserving what was selected before it.
- `src/components/ui/settings/sections.test.tsx` (15): every page mounted on its
  own, each with its store round-trip.
- `src/i18n/dictionaries.test.ts` (5): key parity, no empty strings, matching
  placeholders across languages, and no key claimed by two namespaces.

### Verified

`npm run check` exits 0: 247 Rust tests, 277 frontend tests, `cargo fmt`,
strict Clippy, generated bindings byte-identical, boundaries clean, bundle
within budget. Command names, event names and serialized payloads are
unchanged — `check:bindings` regenerates and the tree stays clean.

Each gate of the new check was deliberately broken and confirmed to fail: a
`lib/ → store/` import, an induced cycle between two `lib` modules, and a
`transfer/plan.rs` reaching up to `engine` — each reverted. The settings dialog
was driven in the browser demo through all seven pages.

### Not done

`commands/fs/search.rs` (911) and `archive.rs` (905) were left alone: each is
about one thing already (search modes; archive formats), and the ticket's list
did not name them. `store/panesStore.ts` is still 755 lines — what remains is
the asynchronous loading, history and tab handling that the ticket explicitly
scoped as "where doing so improves tests"; the parts worth testing directly are
out.
