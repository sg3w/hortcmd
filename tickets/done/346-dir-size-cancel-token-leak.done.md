# TICKET-346: Fix cancel-token leak when dir_size's capability check fails

Discovered during a full-codebase review (2026-09-22).

## Problem

In `src-tauri/src/commands/fs/size.rs` (around lines 149-152),
`cancel::register(KIND, &token)` runs only after `caps::require(...)?`
succeeds; if the capability check fails (the `?` returns early), any cancel
flag pre-created by a racing `cancel_dir_size(token)` call (per `cancel.rs`'s
documented "cancel before start" support) is never removed via
`cancel::unregister`.

A caller that invokes `cancel_dir_size(token)` for a request whose `dir_size`
call fails capability validation (e.g. a stale/racing UI state pointing at a
now-remote or now-invalid address) leaves an orphaned `Arc<AtomicBool>` entry
keyed by that token in the process-lifetime `CANCELS` map forever — a slow,
unbounded leak proportional to how often this race occurs. Related to ticket
335's broader cancel-map leak but a distinct trigger path worth its own fix.

## Required change

- Register the cancel token before the `caps::require` check (or wrap the
  whole function body so `cancel::unregister` always runs via a guard/
  `defer`-style pattern), so every registered token is guaranteed to be
  cleaned up.

## Verification

- Test: call `cancel_dir_size(token)` for a request that then fails
  capability validation; assert the token is cleaned up rather than leaked.
- `npm run check`.

## Resolution (2026-09-25)

Folder-size requests now register cancellation before validation and use a drop guard to clean up on every exit. Regressions cover an offline capability refusal after a pre-armed cancel and a successful early-cancelled command, including clean token reuse.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
