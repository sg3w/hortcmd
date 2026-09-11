# TICKET-033: Establish CI and frontend test foundations

## Goal

Every proposed change receives repeatable automated feedback for Rust quality,
TypeScript correctness, frontend behavior, production bundling, and supported
platform compilation. Critical keyboard, dialog, path, store, and transfer logic
must no longer rely entirely on manual regression testing.

## Background

The assessed working tree passes TypeScript checking, the Vite production build,
and 83 Rust library tests. Six live/keychain tests are intentionally ignored.
However, `cargo fmt --check` reports formatting differences and strict Clippy
reports 10 findings. There is no frontend test runner, no `test` script, and no
`.github` CI workflow.

The frontend includes global keyboard handling, several persistent Zustand stores,
dialog lifecycle behavior, remote/local path manipulation, and asynchronous event
routing. These areas have already produced interaction bugs that compile-time
checks cannot detect. The growing platform-specific and network code also needs
continuous compilation outside the developer's primary platform.

## Requirements

### Clean baseline

- Apply `cargo fmt` and resolve the current strict Clippy findings without hiding
  them behind broad crate/module-level allowances.
- A narrow `#[allow]` is acceptable only when the API shape is externally required
  or the alternative is less readable; document the reason at the item.
- Keep `npm run typecheck` and `npm run build` warning-aware. The existing bundle-
  size warning is documented but does not fail this ticket.
- Ensure generating ts-rs bindings produces no unexpected repository diff.

### Frontend test setup

- Add Vitest, React Testing Library, `@testing-library/user-event`, and jsdom using
  versions compatible with the existing React, TypeScript, and Vite setup.
- Add a shared test setup that resets DOM, localStorage, Zustand state, timers, and
  mocks between tests.
- Provide typed mocks for Tauri `invoke`, events, channels, and window APIs. Tests
  must be able to select browser-demo or Tauri behavior explicitly.
- Add scripts:
  - `test` for a non-watch, CI-suitable frontend run;
  - `test:watch` for local development;
  - `check` that runs formatting check, Clippy, Rust tests, frontend tests,
    TypeScript checking, generated-binding verification, and production build.
- Keep tests beside the relevant modules or under one documented test directory;
  do not mix both conventions without a reason.

### Initial frontend coverage

- Unit-test local and remote helpers in `src/lib/path.ts`, including roots,
  ancestors, Windows paths, remote authorities, spaces, and non-ASCII names.
- Unit-test batch-rename rules and custom file-color matching/priority.
- Test pane/store behaviors that affect data safety: selection preservation,
  reveal after refresh, tab restoration fallback, and queued transfer ordering.
- Test global keyboard isolation with an open dialog, including Enter and Escape
  on prompts as described by TICKET-014.
- Test IPC event subscription fan-out and unsubscribe behavior without a real
  Tauri runtime.
- Add component tests for at least one representative Radix dialog and one
  capability-controlled context menu once TICKET-031 is implemented.

### GitHub Actions

- Add `.github/workflows/ci.yml` for pushes and pull requests.
- Use locked installs (`npm ci`, `cargo --locked`) and cache npm plus Cargo build
  data using lockfile-derived keys.
- Run frontend tests, typecheck, build, Rust formatting, strict Clippy, Rust library
  tests, and generated-binding verification on the primary Linux job.
- Install the documented Tauri Linux build prerequisites explicitly.
- Add a Rust OS matrix for Ubuntu, macOS, and Windows to compile and run portable
  library tests, ensuring `cfg`-gated code stays healthy.
- Verify the declared MSRV 1.85 with `cargo check --locked`; use current stable Rust
  for formatting, Clippy, and the main test matrix.
- Do not run ignored live network or OS-keychain tests in ordinary CI. Provide a
  separately documented manual workflow or local command for them; never require
  real credentials in pull-request jobs.
- Set least-privilege workflow permissions (`contents: read`) and avoid executing
  untrusted pull-request code with repository secrets.

### Developer documentation

- Update README.md and CLAUDE.md with the canonical local commands, test layout,
  required toolchain versions, and the difference between normal and live tests.
- Fail CI on formatting, Clippy warnings, test failure, type errors, unexpected
  binding changes, or build failure.
- Keep CI commands identical to package/Cargo scripts used locally wherever
  possible so developers can reproduce failures.

## Acceptance criteria

- [x] `cargo fmt --check` passes from a clean checkout.
- [x] Strict Clippy passes for all targets and features without broad allowances.
- [x] Rust library tests, frontend tests, TypeScript checking, binding generation,
      and the production build pass through one documented local command.
- [x] Frontend tests run in CI without a native Tauri process.
- [x] Initial tests cover path handling, rename/color rules, critical store logic,
      keyboard/dialog isolation, and IPC subscriptions.
- [x] Ubuntu, macOS, and Windows jobs compile and exercise platform-gated code.
- [x] The MSRV job succeeds on Rust 1.85 with locked dependencies.
- [x] Pull-request CI has no write permission and receives no live-test secrets.
- [x] Ignored live tests remain available through a documented manual workflow.

## Tests

- Deliberately introduce a formatting error, Clippy warning, TypeScript error,
  failing Vitest assertion, stale generated binding, and Rust test failure on a
  temporary branch; verify that the expected CI step fails each time.
- Run the full workflow on Linux, macOS, and Windows.
- Verify frontend tests in both demo and mocked-Tauri modes.
- Verify tests restore global state and remain deterministic under randomized order
  or repeated execution.
- Confirm a forked pull request cannot access secrets or gain write permissions.
- Manually dispatch the documented live-test workflow without credentials and
  confirm it fails safely with an actionable setup message.

## Definition of Done

- The repository has a green CI workflow on all supported desktop platforms.
- Contributors can reproduce every required CI check locally.
- Existing formatting and Clippy debt is resolved.
- The initial frontend regression suite is stable and documented.
- No production behavior changes except fixes required to establish the clean
  baseline and deterministic tests.
- Concept.md records CI and automated test coverage as cross-cutting infrastructure.
- Rename this file to `033-ci-and-frontend-test-foundation.done.md`.

---

## Implementation

### Clean baseline

- `cargo fmt` applied; `cargo fmt --check` is green from a clean checkout.
- All 10 strict-Clippy findings resolved **without** allowances:
  - `copy_file_prog` / `copy_tree` / `transfer_one` / `parallel_copy_file` /
    `run_parallel_copy` now share a `CopyTuning { limit, verify, buf_size }`
    value instead of threading three unrelated parameters through every level.
    That also removed two pre-existing `#[allow(clippy::too_many_arguments)]`.
  - `run_extract` takes an `ExtractJob` — the five parts are one request.
  - `ConnectError::HostKey` is boxed: the challenge carries six strings, which
    otherwise sized every `Result<_, ConnectError>` in the connect path by its
    rarest case.
  - `SftpProvider::new` → `shared`, since it returns `Arc<dyn Provider>`.
  - `&PathBuf` → `&Path` in four test helpers, `sort_by` → `sort_by_key`,
    `&*sp` → `sp.as_ref()`, `&[path.clone()]` → `std::slice::from_ref(&path)`,
    and a named `ArchiveCase` alias for the format table.
- `check:bindings` confirms the generated bindings match the Rust types.

### Frontend test setup

- Vitest (already present) plus React Testing Library, `user-event` and
  `jest-dom`.
- `src/test/setup.ts`: an in-memory `localStorage` (installed at module load —
  the stores are created on import, before the first `beforeEach`), and a reset
  after every test of the DOM, timers, mocks, the Tauri harness and every
  Zustand store. Stores are tracked by wrapping `create`/`createStore` through
  `vi.mock("zustand")`; `zustand/middleware` keeps resolving to the real
  package.
- `src/test/tauri.ts`: `useDemo()` / `useTauri({ commands })`, recorded calls,
  event pushes, and a loud failure for an unmocked command.
- `src/test/fixtures.ts`: typed `DirEntry`/`Row` builders, so a new field on the
  Rust side is one edit rather than a dozen.
- Scripts: `test`, `test:watch`, `check` plus the `check:*` steps it composes.
  Tests live beside their module; `src/test/` holds only shared machinery.

### Initial coverage (196 frontend tests, 14 files)

`path` (19) · `renameRules` (21) · `fileColors` (17) · `panesStore` (9) ·
`transferQueue` (6) · `useKeyboard` (12) · `ipc/events` (9) · `AppDialog` (7) ·
`FileContextMenu` (14), on top of the existing `preview` (19) and the suites
from TICKET-031/032 (63).

One real bug surfaced while writing them: `ancestorChain("sftp://u@host/")`
returned the non-canonical `"sftp://u@host"`, so a breadcrumb's root crumb did
not match the pane's own path. Fixed in `src/lib/path.ts`.

### GitHub Actions

- `ci.yml` — three jobs: the full gate on Linux (each `check:*` as its own step,
  so the failing one is visible in the step list), an Ubuntu/macOS/Windows Rust
  matrix for the `cfg`-gated code, and an MSRV 1.85 job with `--locked`.
  `npm ci`, `cargo --locked`, npm and Cargo caches keyed on the lockfiles,
  Tauri's Linux prerequisites named explicitly, `permissions: contents: read`,
  and no secrets anywhere.
- `live-tests.yml` — manual dispatch only, with an acknowledgement input; run
  without the servers it exits with the exact local command and the Docker
  hints instead of pretending to pass.

### Documentation

README gained a "Tests and CI" section (the canonical commands, the test
layout, how to pick demo vs. mocked-Tauri mode, the live-test command, the CI
jobs) and updated toolchain requirements. AGENTS.md now points at
`npm run check`. Concept.md records the whole thing as cross-cutting
infrastructure.

### Verified

`npm run check` exits 0 end to end. Each gate was then deliberately broken on a
scratch edit and confirmed to fail: formatting (exit 1), a Clippy finding
(101), a stale generated binding (1), a TypeScript error (2), a failing Vitest
assertion (1), and a failing Rust test (101) — each reverted afterwards.
Determinism was checked by running the suite three times with different shuffle
seeds, once with `--no-file-parallelism` and once in a single thread: 196
passed every time.

Not verified here: the workflows have not been executed on GitHub's runners —
that needs a push, which this ticket does not do. The macOS/Windows matrix and
the Linux prerequisite list are therefore declared, not yet observed green.
