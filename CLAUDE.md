# CLAUDE.md

# hortcmd

hortcmd is a cross-platform file manager in the style of Total Commander, built with **Rust**, **Tauri v2**, **React**, and **TypeScript**.

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

Check after changes:

```bash
cargo check
npm run typecheck
npm run build
```

For changes to IPC types additionally:

```bash
npm run gen:types
```

## Priorities

1. Stability
2. Data safety
3. Maintainability
4. Performance
5. Cross-platform compatibility
