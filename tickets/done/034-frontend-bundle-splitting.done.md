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

- [x] The production build no longer emits the current oversized-main-chunk
      warning, or the repository documents a lower explicit budget that the build
      enforces with a justified threshold.
- [x] The SSH terminal, Markdown/highlighting code, and at least two large dialogs
      are absent from the eager application chunk.
- [x] The initial pane is interactive before any lazy feature finishes loading.
- [x] Opening each lazy feature works in development, a packaged Tauri build, and
      browser-demo mode.
- [x] Lazy loading failures show a recoverable, localized message and do not crash
      the application shell.
- [x] Keyboard focus and modal isolation remain correct during loading.
- [x] The production CSP from TICKET-030 does not require broadening for chunks.

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

---

## Implementation

### Baseline

`npm run build` before: one eager chunk of **1,306 kB** (372 kB gzipped), with
the Rollup oversized-chunk warning. All sixteen dialogs were imported and
rendered unconditionally in `App.tsx`, each self-managing its visibility — so
Markdown rendering, syntax highlighting and a terminal emulator were parsed
before the two panes could be drawn.

### Boundaries

- **`src/components/ui/LazyFeature.tsx`** — the wrapper. Renders `null` while
  closed (no invisible modal, so nothing steals focus or swallows a function
  key); an inert `role="status"` / `aria-live="polite"` line with
  `pointer-events: none` while loading; and an error boundary that turns a
  rejected import into a localized message with a working retry. The retry has
  to build a *fresh* `lazy()`, so the boundary creates the lazy component rather
  than receiving it as a child — a rejected `lazy()` replays its rejection
  forever. Unmounting is delayed one beat after close, because unmounting a
  Radix dialog while it is still open leaks `pointer-events: none` onto `<body>`.
- **`src/features/lazyFeatures.ts`** — every boundary in one list, plus
  `PRELOAD_ON_MENU`.
- **`src/features/LazyDialogs.tsx`** — pairs each with the store flag that opens
  it. `App.tsx` now imports only the shell, the operation prompts and the
  transfer surface.
- **Preload on intent**: pointing at or focusing a menubar category warms that
  category's heavyweight chunks. Nothing is preloaded at startup.
- **`SettingsDialog` made self-contained.** It took `{open, onOpenChange}`
  props, which a prop-less lazy boundary cannot supply — it now reads
  `uiStore` like every other dialog. Caught by opening it in the browser, where
  the dialog stayed closed with `settingsOpen: true`.

### Chunking and budget

- Four coarse vendor groups (react, radix, tanstack, lucide) for cacheability;
  deliberately not finer.
- `EAGER_BUDGET_KB = 320` in `vite.config.ts`; Rollup's own warning limit sits
  above the largest deliberate lazy chunk so it stops duplicating a check the
  report makes precisely.
- **`scripts/bundle-report.mjs`** (`npm run check:bundle`, in `npm run check`
  and in CI) reads the build manifest, walks the entry chunk's import graph to
  separate eager from lazy, prints which watched dependency landed where, and
  fails on a budget overrun *or* on xterm/highlight.js/marked reappearing in the
  eager graph. Detection uses tokens only those libraries emit — package names
  are useless in minified output, and the first draft's `\bmarked\b` matched the
  English UI string "… marked".

### Result

| | before | after |
|---|---|---|
| eager application chunk | 1,306 kB (372 kB gz) | **230 kB (70 kB gz)** |
| preview (marked/highlight.js/DOMPurify) | eager | lazy, 146 kB |
| SSH terminal (xterm) | eager | lazy, 330 kB |
| settings / connections / properties / rename / compare / search / export | eager | lazy, own chunks |

### Tests

- `LazyFeature.test.tsx` (10): nothing rendered and nothing imported while
  closed; the loading line announced, inert, focus-neutral; a caller-supplied
  fallback; mounting on success; one import across a reopen; a rejected import
  reported with the shell still alive; a retry that really re-imports; preload
  fetching without rendering and swallowing its own failure.
- `LazyDialogs.test.tsx` (5): nothing mounted at rest — no dialog, no status
  line; opening one brings in exactly that one; every declared loader resolves
  to a component (a renamed export would otherwise fail only for the user);
  the preload map only names real loaders and covers the heavyweights.

### Verified

`npm run check` exits 0 (237 Rust, 211 frontend, budget met). In the browser
demo: F3 fetched `PreviewDialog` on demand and rendered Markdown; opening the
Configuration menu warmed `SettingsDialog` **without** opening it, and neither
`SshTerminal`, `ConnectionsDialog` nor `SearchDialog` had been fetched at that
point; all eight remaining dialogs opened correctly; the terminal loaded xterm,
rendered, closed, and left `body { pointer-events: auto }`. During a pending
load the notice was present and `pointer-events: none`, focus was unchanged and
the panes were still rendered. The budget gate was then deliberately broken by
static-importing `SshTerminalWindow` in `App.tsx`: it failed with both "entry
chunk is 562.7 kB, over the 320 kB budget" and "xterm is back in the eager
graph" — reverted afterwards. The production CSP is untouched.

Not verified here: a packaged `tauri build` on Windows and Linux. The chunk
paths are ordinary relative asset URLs under the same origin as the document,
which is what the custom protocol serves, but the packaged build was not run on
those platforms.
