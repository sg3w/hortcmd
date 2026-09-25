# TICKET-338: Make useTauri() actually force the native IPC path in tests

Discovered during a full-codebase review (2026-09-22).

## Problem

`hasTauri` in `src/ipc/invoke.ts` (lines 13-14) is a top-level `const`
evaluated once when the module is first imported/evaluated in a test's
module graph. `useTauri()` (`src/test/tauri.ts`, lines 85-101) only sets
`globalThis.__TAURI_INTERNALS__` and the command table — it never causes
`hasTauri` to be re-read. Every `hasTauri`-gated domain function (the
majority of them) therefore keeps taking the demo branch inside a
`useTauri()` test unless the test additionally either (a) calls
`vi.resetModules()` and dynamically re-imports the module under test *after*
`useTauri()`, or (b) adds
`vi.mock("@/ipc/invoke", async orig => ({...(await orig()), hasTauri: true}))`.

Grepping every `*.test.tsx` that calls `useTauri()`: ~20 files carry the
`vi.mock` workaround and ~14 more use the `vi.resetModules()` + dynamic
`import()` pattern; the one file relying on neither (`PluginFrame.test.tsx`)
only happens to work because the plugin IPC functions it exercises call
`invoke()` unconditionally. Nothing enforces either workaround — a new
component test that calls `useTauri({commands: {...}})` and simply renders a
statically-imported component (the natural way to write an RTL test) will
silently exercise the demo fallback instead of the mocked native path. If the
demo fallback happens to return a similarly-shaped value, the test can pass
while never having invoked the mocked command at all — a false-positive test
that would not catch a real regression in the native code path.

## Required change

- Make `useTauri()` itself force the reactive part of the contract: have
  `invoke.ts` check `"__TAURI_INTERNALS__" in window` per-call (or via a
  mutable flag `useTauri()`/`useDemo()` set directly) instead of a load-time
  `const`, so every consumer of `src/test/tauri.ts` gets correct behavior for
  free instead of needing an undocumented per-file workaround.
- Once fixed, the ~34 files carrying the workaround can drop it (follow-up,
  not required for this ticket).

## Verification

- New test: call `useTauri({commands: {...}})` and render a
  statically-imported component with no `vi.mock`/`resetModules` workaround;
  assert the mocked command is actually invoked.
- `npm run check`.
