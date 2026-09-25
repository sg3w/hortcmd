# TICKET-322: Kill a native plugin's child processes on Windows too

Discovered during a full-codebase review (2026-09-22).

## Problem

In `src-tauri/src/plugins/native_host.rs` (around lines 129-142), when a
native plugin helper fails to shut down cooperatively within the 300ms grace
period, only the immediate helper process is killed on Windows — any child
processes the loaded native library spawned are never terminated.

On Unix, `libc::kill(-(id as i32), SIGKILL)` kills the whole process group
including descendants. On Windows this branch is `#[cfg(unix)]`-only; the
Windows path falls through to bare `child.start_kill()`, which only
terminates the top-level helper. Worse, `native_processes.remove(&id)` runs
immediately after `supervise()` returns, so even the later app-exit
`shutdown()` (which does call `taskkill /T /F`) no longer has this pid to
clean up — the orphaned descendant runs indefinitely.

This same process-group-kill sequence (unix `kill(-pid, SIGKILL)` + Windows
`taskkill /PID <pid> /T /F`) is implemented three separate times —
`command_line.rs::ProcessGroup::terminate` (lines 167-186), and twice in
`native_host.rs` (`supervise()`'s cleanup, lines 133-141, and `shutdown()`,
lines 261-277) — each with its own `unsafe` block, which is exactly how this
gap in one copy went unnoticed.

## Required change

- Extract a single `fn kill_process_group(pid: u32)` (e.g. in a small shared
  `process` helper module) covering both unix `kill(-pid, SIGKILL)` and
  Windows `taskkill.exe /PID <pid> /T /F`, and use it from all three call
  sites, so the Windows fix lands in one place and the duplication can't
  drift again.

## Verification

- Test/manual verification on Windows: a native plugin helper that spawns a
  child process and hangs on close; assert both the helper and its child are
  gone after the timeout.
- `npm run check`.
