# AGENTS.md

# hortcmd

hortcmd is a cross-platform file manager in the style of Norton Commander, built with **Rust**, **Tauri v2**, **React**, and **TypeScript**.

## Principles

- Prefer **Rust** for all file-system, system, and backend functionality.
- The frontend serves solely for presentation and user interaction.
- Communication between frontend and backend happens exclusively via **Tauri commands**.
- Keep the existing architecture and project structure.
- Changes should be small, comprehensible, and maintainable.

## Architecture

```
src/          React + TypeScript
src-tauri/    Rust + Tauri
```

Business logic belongs in the backend. The frontend contains no duplicated file-system logic.

## Backend

- Robust and idiomatic Rust code
- Handle errors via `Result`, no unnecessary `panic!`
- Account for platform differences (Windows, macOS, Linux)
- Move complex logic into services, keep commands as thin as possible
- Export IPC data types via **ts-rs**

## Frontend

- React function components
- Use TypeScript strictly
- Zustand for state management
- TanStack Table and Virtual for large directories
- Radix UI for dialogs and menus
- Tailwind CSS for styling

## Code quality

Prefer:

- small, well-structured functions
- expressive names
- as little code duplication as possible
- clean error handling
- performance without unnecessary optimization

Avoid:

- unnecessary abstractions
- large files with many responsibilities
- logic in the frontend that belongs in the backend

## Development

Each ticket must be implemented on its own Git branch, created from the current
local `main` before starting work. Name it `codex/ticket-<number>-<short-description>`.
Keep changes scoped to that ticket; use a separate branch for each additional ticket.

After completing a ticket and passing `npm run check`, create a local Git commit
containing only the changes for that ticket. Switch to `main`, merge the ticket
branch into it, and remain on `main` until starting the next ticket. If resolving
merge conflicts changes the code, run `npm run check` again on the merged result.
Do not merge unfinished tickets. Do not push commits or branches.

Check after changes — one command, the same one CI runs:

```bash
npm run check
```

It runs, in order: `cargo fmt --check`, strict Clippy (all targets and
features, warnings are errors), `cargo test --locked`, a regeneration of the
ts-rs bindings that must leave the tree clean, `tsc --noEmit`, the Vitest suite
and the production build. Any of them can also be run on its own — see
`check:*` in `package.json`.

For changes to IPC types, regenerate and commit the bindings:

```bash
npm run gen:types
```

## Tests

Tests live beside the module they cover; `src/test/` holds only the shared
machinery (setup and reset, the typed Tauri stand-ins, fixtures). A frontend
test picks its mode explicitly — `useDemo()` for the browser build's demo
fallbacks, `useTauri({ commands })` for a mocked runtime — so no native process
is ever needed.

Protocol and keychain tests are `#[ignore]`d and never run in ordinary CI; see
the "Tests and CI" section of README.md for the local command.

## Priorities

1. Stability
2. Data safety
3. Maintainability
4. Performance
5. Cross-platform compatibility
