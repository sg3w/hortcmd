# TICKET-344: Fix or drop tags.rs's unreachable non-macOS fallback

Discovered during a full-codebase review (2026-09-22).

## Problem

The module doc in `src-tauri/src/commands/fs/tags.rs` (lines 5-9) claims "On
non-macOS platforms the commands return empty values or a clear error," but
that `#[cfg(not(target_os = "macos"))]` code (lines 38-42, 59-63) is
unreachable: `Capability::FinderTags` is added to `local_caps()` only `if
cfg!(target_os = "macos")` (`vfs/caps.rs` line ~248-251) and is never in
`remote_core_caps()`, so `caps::require(&path, Capability::FinderTags)` in
`get_tags`/`set_tags` always fails first on non-macOS, for every address,
before the platform branch is ever reached.

Not a crash, but a maintainer reading the doc comment or the
`#[cfg(not(macos))]` arm will believe `get_tags` can return `Ok(vec![])` on
Linux/Windows; it cannot — it always short-circuits with
`CAPABILITY_UNSUPPORTED`/`CAPABILITY_OFFLINE`, making the fallback branch
dead code that could silently diverge from the doc during a future refactor.

## Required change

- Either drop the redundant per-platform fallback logic (since
  `caps::require` already gates it) and update the header comment to say so,
  or keep it only as a documented defense-in-depth note rather than implying
  it's reachable behavior.

## Verification

- `cargo check`/`npm run check` after the cleanup; confirm the module doc
  comment matches actual behavior.

## Resolution (2026-09-27)

The module doc no longer claims a non-macOS behaviour that cannot happen. It
now states the actual rule: `Capability::FinderTags` is in `local_caps()` on
macOS alone and in no remote capability set, so `caps::require` refuses every
address on the other platforms before any tag code runs, and the
`#[cfg(not(macos))]` arms exist only because the compiler needs a value there.

Those arms no longer invent an answer. `get_tags` returned `Ok(vec![])` — the
exact divergence the ticket describes — and `set_tags` returned a prose
`String` error; both now call one shared `unreachable_without_macos`, which
re-runs `caps::require` and hands back the very refusal the gate above already
produced (falling back to `capability.unsupported` if that ever succeeded).
There is one answer for this platform instead of three, so a future refactor
cannot let them drift apart.

The arms are not compiled on macOS, so they were type-checked by temporarily
flipping the two `cfg`s to `all()`/`any()` and running `cargo check`; the
change was reverted before the commit.

Validation: `npm run check` passed on macOS.
