# TICKET-352: Log a failure to apply the native menu language

Discovered during a full-codebase review (2026-09-22).

## Problem

`let _ = imp::apply(&handle, &lang);` in `set_menu_language`
(`src-tauri/src/menu.rs`, around lines 247-251, inside the main-thread
closure) silently discards any `tauri::Result` failure from rebuilding the
native macOS menu — no log, no signal back to the frontend.

If `imp::apply` ever fails (e.g. a future `muda`/Tauri menu-build error), the
user's language switch silently no-ops on macOS — the native menu stays in
the old language with zero diagnostic trail, unlike the analogous
blocked-navigation path in `lib.rs` (line 205) which at least `eprintln!`s in
debug builds. Low impact (UI-only, no data/security risk) but a real
swallowed-error path.

## Required change

- At minimum `eprintln!` the error in debug builds (mirroring the
  navigation-block pattern already used in `lib.rs`), so a real failure isn't
  invisible during development.

## Verification

- `npm run check`; manual confirmation the debug build logs a message if
  `imp::apply` is made to fail (e.g. temporarily).
