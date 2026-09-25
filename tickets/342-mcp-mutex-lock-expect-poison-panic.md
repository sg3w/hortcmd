# TICKET-342: Tolerate poisoned locks in MCP audit/confirm/config code

Discovered during a full-codebase review (2026-09-22).

## Problem

`src-tauri/src/mcp/audit.rs` (lines 93, 112, 121), `src-tauri/src/mcp/
confirm.rs` (lines 80, 92, 97, 114, 119, 137, 167, 175, 194, 204), and
`src-tauri/src/mcp/mod.rs` (lines 197, 243) use `Mutex::lock().expect(...)`,
which panics permanently on poisoning, whereas the rest of the module
(`server.rs`, `session.rs`) consistently uses
`.unwrap_or_else(|e| e.into_inner())` to tolerate a poisoned lock.

`audit::record()` runs on literally every tool call (allowed, denied, or
failed); `confirm::ask()`/`answer()` run on every write/delete confirmation.
If any future change (or an unexpected panic, e.g. from an allocation
failure) panics while holding one of these locks, the mutex is poisoned and
*every subsequent* `.expect(...)` call on it panics too — turning one
transient bug into a permanent crash for that subsystem (audit logging,
confirmation dialogs, or MCP config reads/writes) for the remainder of the
process's life, directly against the project's #1 priority (Stability). The
rest of the module was clearly written with this exact class of failure in
mind, so this is a missing-invariant-on-a-sibling case rather than a
deliberate choice.

## Required change

- Replace `.lock().expect(...)` with `.lock().unwrap_or_else(|e|
  e.into_inner())` in `audit.rs`, `confirm.rs`, and `mod.rs` to match the
  pattern already used elsewhere in the same module.

## Verification

- `cargo clippy`/`npm run check` after the change; optionally a test that
  poisons one of these mutexes (panic in a locked scope) and asserts a
  subsequent call still succeeds instead of panicking.
