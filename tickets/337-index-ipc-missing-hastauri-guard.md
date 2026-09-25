# TICKET-337: Guard the remaining index IPC calls the way indexCreate does

Discovered during a full-codebase review (2026-09-22).

## Problem

In `src/ipc/domains/index.ts` (around lines 6-15), `indexCreate` explicitly
guards `!hasTauri` with a clear rejection ("Native runtime required"), but
the sibling functions in the same file — `indexUnlock`, `indexStep`,
`indexCancel`, `indexControl`, `indexQuery` — call `invoke(...)`
unconditionally, with no demo fallback and no `hasTauri` guard.

`src/components/ui/SearchIndexPanel.tsx` calls `indexUnlock`, `indexStep`,
`indexCancel`, `indexControl`, `indexQuery` directly, without its own
`hasTauri` gate. In the browser/demo build, any of these calls goes through
`invoke()` in `src/ipc/invoke.ts`, which dynamically imports
`@tauri-apps/api/core`; with no `__TAURI_INTERNALS__` on `window` that call
throws an unstructured, low-level `TypeError` rather than the app's
structured `AppError` (`code` + `params`). This breaks the "errors carry a
code + safe params" contract, and is inconsistent with `indexCreate` in the
very same file.

The same asymmetry (a guarded function next to unguarded ones that reach real
UI) also exists in `net.ts` (`netConnect`/`netTrustHostKey`/`sshOpenShell` vs.
their guarded siblings) and `recovery.ts`/`undo.ts` — worth the same audit
while fixing this one.

## Required change

- Give `indexUnlock`, `indexStep`, `indexCancel`, `indexControl`,
  `indexQuery` the same `if (!hasTauri) return Promise.reject(...)` (or
  equivalent demo-safe rejection) that `indexCreate` already has.
- Audit `net.ts` and `recovery.ts`/`undo.ts` for the same asymmetry and apply
  the same fix where a guarded function has unguarded siblings reachable
  from real UI.

## Verification

- Test: call each of the five index functions in a demo-mode/no-Tauri
  environment; assert each rejects with a structured `AppError`, not a raw
  `TypeError`.
- `npm run check`.
