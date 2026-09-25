# TICKET-325: Gate save_selection/load_selection on a capability check

Discovered during a full-codebase review (2026-09-22).

## Problem

`save_selection` and `load_selection` in
`src-tauri/src/commands/fs/selection.rs` (around lines 131-151 and 156-190)
never call `caps::require`, even though `save_selection` performs the exact
same write pattern (`provider.write(p, bytes)` after an overwrite check) as
`write_text_file` in `file.rs:41-54`, which explicitly gates on
`caps::require(&path, Capability::Create)` first. `vfs::resolve` itself
performs no capability enforcement, so nothing else in the call chain
protects these two commands.

A `path` that resolves to a location the capability matrix would refuse for
writes (or reads) still gets written/read because the gate every
structurally-identical sibling command applies is simply absent here.

## Required change

- Add `caps::require(&path, Capability::Create)` in `save_selection` and an
  analogous read-capability check (e.g. `Capability::Preview`) in
  `load_selection`, matching the pattern used throughout `file.rs`.

## Verification

- Test that `save_selection`/`load_selection` refuse a mocked provider whose
  capability set lacks the relevant capability, the same way `file.rs`'s
  commands do.
- `npm run check`.

## Resolution (2026-09-25)

Selection saves require Create and loads require Preview before provider resolution or file I/O. Tests verify the appropriate capability refusals, and the existing local selection round-trip and validation tests pass.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
