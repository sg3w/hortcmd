# TICKET-003: Make GIT support and folder opening asynchronous

## Goal
The performance when opening directories in `hortcmd` should be optimized. The initialization of the GIT support must no longer block the main thread and must run asynchronously in the background.

## Background
Currently opening a folder leads to noticeable delays or a brief freeze of the user interface (UI freeze), because the check for GIT support and the reading of the repository status happen synchronously. Especially with large repositories this severely impairs the user experience.

## Requirements
- Decouple folder opening from the GIT status query.
- Implement an asynchronous workflow for all GIT-related initialization processes.
- Ensure the UI stays responsive during loading.

## Acceptance criteria
- [x] Opening a folder happens immediately; the application no longer freezes.
- [x] GIT support is loaded in the background and updates the display/functionality as soon as the data is available.
- [x] For very large repositories, navigation in `hortcmd` stays smooth.
- [x] No race conditions occur when a folder is switched quickly while a GIT query for the previous folder is still running.

## Implementation
The implementation is done in the backend by moving the GIT logic into an asynchronous task (e.g. via `tokio::spawn` in Rust).

Backend: adjust the command handlers in `src-tauri/src/commands/` (specifically the functions for directory handling) so they no longer wait blocking on the GIT process. The results are returned to the frontend via Tauri events (`emit`).

Frontend: implement a listener that reacts to the `git-support-ready` event and updates the corresponding state in the store to unlock the GIT features.

## Tests
- Open a small directory without GIT (check for speed).
- Open a large Git repository (check that the UI stays operable).
- Fast switching between different folders during the initialization phase.

## Definition of Done
- [x] Code compiles
- [x] Tests pass
- [x] Existing functionality unchanged
- [x] Documentation updated
- [x] Rename the ticket to `*.done.md`

## Implementation note
- Backend (`src-tauri/src/commands/fs/git.rs`): `git_status` (command) → `git_status_watch`
  (async command). The computation runs via `tauri::async_runtime::spawn_blocking` in the
  background; the result is sent to all windows via the event `git-support-ready` (payload `GitStatusEvent`
  = path + `GitStatus`). The command itself does not block.
- Frontend (`src/ipc/client.ts`): `gitStatus()` → `requestGitStatus()` (fire-and-forget)
  + `onGitStatusReady()` (event listener), following the existing pattern of
  `onFsChanged`/`fs-changed`.
- Store (`src/store/panesStore.ts`): `loadGit` now only requests (no more waiting);
  new action `applyGitStatus(path, git)` applies the result to all tabs of both
  panes with a matching path (events carry no side info) and discards
  results if GIT was disabled in the meantime → no race conditions.
- `App.tsx` wires the listener globally (the store stays Tauri-decoupled).
- Verified: `cargo check`, `cargo test`, `npm run typecheck`, `npm run build` as well as
  manually in the browser preview (demo mode): the branch badge/status colors still load
  correctly, even after fast folder switching, no console errors.
