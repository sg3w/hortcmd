# TICKET-034: Split the frontend bundle by feature boundary

## Goal

Reduce initial WebView startup cost and the size of the main JavaScript chunk by
loading heavyweight, infrequently used features only when the user opens them.
The application must retain its keyboard-first behavior and must not introduce
visible loading instability.

## Background

The current production build succeeds but reports a main JavaScript chunk of
approximately 1.16 MB before compression and 331 KB after gzip. Markdown parsing,
syntax highlighting, terminal emulation, connection management, settings, search,
comparison, properties, and archive dialogs are all reachable from the initial
frontend dependency graph. `src/ipc/client.ts` already demonstrates that dynamic
imports work in both browser-demo and Tauri modes, but feature-level React code is
not currently split.

This is primarily a startup and maintainability improvement. It must follow the
security constraints from TICKET-030: lazy chunks are application assets, not a
reason to weaken the production Content Security Policy.

## Requirements

- Capture a reproducible baseline for emitted chunk sizes, WebView-ready time,
  and time to first usable file pane on a representative development machine.
- Add explicit lazy boundaries for heavyweight features that are not required to
  render the initial two-pane shell, including at least:
  - Markdown and syntax-highlighted preview support;
  - the SSH terminal and xterm dependencies;
  - connection management;
  - settings, search, comparison, and properties dialogs where beneficial.
- Keep core navigation, pane rendering, file operations, operation prompts, and
  the transfer-status surface in the eager entry chunk.
- Use stable, accessible loading and error fallbacks. Loading a feature must not
  steal focus, swallow a function-key action, or leave an invisible modal active.
- Preload a chunk after strong user intent, such as opening the relevant menu,
  while avoiding unconditional startup preloads.
- Configure deterministic Vite chunking only where it improves cacheability and
  does not create circular or excessively fragmented chunks.
- Ensure all generated chunk paths work with Tauri's production custom protocol
  and with Vite's development server on Windows, macOS, and Linux.
- Document a bundle-size budget for the eager entry chunk and a review process for
  intentional exceptions.

## Acceptance Criteria

- [ ] The production build no longer emits the current oversized-main-chunk
      warning, or the repository documents a lower explicit budget that the build
      enforces with a justified threshold.
- [ ] The SSH terminal, Markdown/highlighting code, and at least two large dialogs
      are absent from the eager application chunk.
- [ ] The initial pane is interactive before any lazy feature finishes loading.
- [ ] Opening each lazy feature works in development, a packaged Tauri build, and
      browser-demo mode.
- [ ] Lazy loading failures show a recoverable, localized message and do not crash
      the application shell.
- [ ] Keyboard focus and modal isolation remain correct during loading.
- [ ] The production CSP from TICKET-030 does not require broadening for chunks.

## Tests

- Add build-level assertions or a bundle report that maps major dependencies to
  their emitted chunks and fails when the eager budget regresses.
- Component-test every lazy boundary for loading, success, and rejected-import
  states.
- Run smoke tests that open each split feature through mouse and keyboard entry
  points in browser-demo and packaged Tauri modes.
- Verify a terminal session and a Markdown preview after their chunks load.
- Measure cold and warm startup before and after the change using the documented
  procedure.
- Run the complete checks introduced by TICKET-033.

## Definition of Done

- Heavy optional features are split at intentional, documented boundaries.
- The initial chunk satisfies the agreed budget and startup measurements improve
  or remain neutral on every supported platform.
- Loading and error states are accessible, localized, and regression-tested.
- Bundle composition is inspectable in CI or through one documented local command.
- Concept.md records the frontend loading boundaries and performance budget.
- Rename this file to `034-frontend-bundle-splitting.done.md`.
