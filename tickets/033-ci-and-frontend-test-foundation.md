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

- [ ] `cargo fmt --check` passes from a clean checkout.
- [ ] Strict Clippy passes for all targets and features without broad allowances.
- [ ] Rust library tests, frontend tests, TypeScript checking, binding generation,
      and the production build pass through one documented local command.
- [ ] Frontend tests run in CI without a native Tauri process.
- [ ] Initial tests cover path handling, rename/color rules, critical store logic,
      keyboard/dialog isolation, and IPC subscriptions.
- [ ] Ubuntu, macOS, and Windows jobs compile and exercise platform-gated code.
- [ ] The MSRV job succeeds on Rust 1.85 with locked dependencies.
- [ ] Pull-request CI has no write permission and receives no live-test secrets.
- [ ] Ignored live tests remain available through a documented manual workflow.

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
