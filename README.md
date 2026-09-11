# hortcmd

A file commander in the style of Total Commander, built with **Rust** + **Tauri v2**.
Dual-pane layout, function-key bar (F3–F8), drive and path selection.

Architecture and design details: see [Concept.md](Concept.md).

## Tech stack

| Layer          | Technology                                         |
|----------------|----------------------------------------------------|
| Backend        | Rust (Tauri v2 commands, `std::fs`, `sysinfo`)     |
| FE language    | TypeScript + React                                 |
| Table          | TanStack Table (headless) + TanStack Virtual       |
| State          | Zustand                                            |
| Overlays/menus | Radix UI (shadcn style) – context menu, select     |
| Icons          | Lucide (`lucide-react`)                            |
| Styling        | Tailwind CSS                                        |
| Type sync      | ts-rs (Rust structs → TS types)                    |
| Bundler        | Vite                                               |

## Project structure

```
hortcmd/
├── index.html                  # Entry point (loads src/main.tsx)
├── package.json                # Scripts & dependencies
├── vite.config.ts · tsconfig*  # Build / TS configuration
├── tailwind.config.js · postcss.config.js
├── src/                        # Frontend (React + TS)
│   ├── main.tsx · App.tsx      # Bootstrap + layout shell
│   ├── index.css               # Tailwind + TC theme tokens
│   ├── ipc/                    # invoke client + ts-rs types (bindings/)
│   ├── store/                  # Zustand store (panesStore.ts)
│   ├── features/panel/         # Panel, FileTable, columns, drive/path/status
│   ├── features/commander/     # Keyboard, selection, actions, F-bar
│   ├── components/ui/          # Radix primitives (context menu …)
│   └── lib/                    # format, path, cn
└── src-tauri/                  # Rust backend
    ├── Cargo.toml · build.rs · tauri.conf.json
    ├── capabilities/ · icons/
    └── src/
        ├── main.rs · lib.rs    # Binary entry + command registration
        └── commands/fs.rs      # list_dir, list_drives, home_dir (+ ts-rs)
```

## Security

**SSH host keys** (TICKET-028). Every SSH connection — SFTP browsing and the
integrated terminal, which share one session — authenticates the *server*
before a password, key signature or agent signature is sent. The presented key
is matched against the user's own OpenSSH `known_hosts` files (read only, never
rewritten) and a hortcmd-managed store at `<config>/hortcmd/known_hosts`:

- **Known key** → connects without a prompt.
- **Unknown host** → the connection stops *before* authentication and the
  connection manager shows host, port, key type and the SHA-256 fingerprint.
  *Trust and connect* stores that exact key and retries; cancelling sends
  nothing.
- **Changed key** → hard failure showing both the stored and the presented
  fingerprint, with no one-click bypass. Replacing a trusted key is an explicit
  step in the connection manager (*Drop host key…*), which only clears hortcmd's
  own store and reports — never edits — conflicting entries in your
  `known_hosts`.

A conflict in *either* source blocks the connection; a match in one never
overrides it.

**Failure-safe transfers** (TICKET-029, Settings → File operations, **off by
default**). Normally a copy writes straight to the final name and an accepted
overwrite deletes the old destination first, so a failed or cancelled overwrite
can lose it. With the setting on, each top-level source is copied into a hidden
staging entry *on the destination store* and published with a single rename:

- The final name appears only once the copy is complete and verified.
- An existing destination survives until the replacement is in place — via the
  provider's atomic replace where it has one, otherwise a backup rename that is
  undone if the commit fails.
- A copy-based move deletes its source only after the destination is committed.
- Cancellations and failures remove only the staging entry.

The tradeoff is space: staging needs room for the copy next to the target. Safe
mode never silently degrades — a destination that cannot commit a staged entry
fails the transfer and says so. Remains of an interrupted run
(`.hortcmd-stage-*`, `.hortcmd-backup-*`) are reported, never deleted without
asking: such an entry can hold the only copy of a file.

**WebView trust boundary** (TICKET-030). The window that renders previews is the
same window that can invoke every Rust command, so untrusted file content is
kept away from it in layers:

- **Content Security Policy** in `tauri.conf.json` — production starts from
  `default-src 'self'` with no `http:`, `https:`, `*` or `unsafe-eval`. `data:`
  is allowed for `img-src` only (generated image previews); `'unsafe-inline'` is
  confined to `style-src` (React/Radix inline styles, dynamic dialog sizing).
  `devCsp` is separate and covers only the Vite origin and HMR, so production
  inherits nothing from it.
- **Navigation policy** — the main WebView accepts only the app document
  (`tauri:` / `http://tauri.localhost`, plus `http://localhost:1420` in dev).
  Everything else is refused; blocked attempts are logged in development with
  the URL alone.
- **Links** in rendered Markdown never navigate the WebView: the click is
  swallowed and a validated `http(s)` URL goes to the system browser through
  `open_external`, which re-checks the scheme. `open_path` stays the file entry
  point and is never used as a URL escape hatch.
- **Sanitization** — Markdown is rendered through DOMPurify with an explicit
  element/attribute allowlist and an `http(s)`-or-fragment URL rule (relative
  hrefs are stripped too: they are same-origin and would otherwise replace the
  running app). Highlighted source is escaped by highlight.js or by hand.

## Requirements

- **Rust**: stable for day-to-day work; the declared minimum is **1.85**
  (`rust-version` in `src-tauri/Cargo.toml`, verified by its own CI job).
  Install via https://rustup.rs →
  `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js** ≥ 20 (CI uses 22)
- Tauri CLI: `npm run tauri` (via devDependency)
- Linux additionally needs Tauri's build prerequisites — the exact package list
  is in `.github/workflows/ci.yml`, which is what CI installs.

## Development

```bash
npm install
npm run dev            # frontend only in the browser (demo data, no Rust)
npm run tauri dev      # native app: Vite + Rust backend
```

## Scripts

```bash
npm run check          # everything CI runs, in the same order
npm run typecheck      # tsc --noEmit
npm run test           # vitest, non-watch (CI mode)
npm run test:watch     # vitest in watch mode
npm run build          # frontend production build
npm run gen:types      # ts-rs: Rust structs → src/ipc/bindings/*.ts
npm run tauri build    # build native app
```

> Without a running Tauri runtime (pure browser) the IPC client automatically
> uses demo data – handy for layout work without a Rust build.

## Tests and CI

`npm run check` is the whole gate, and it is exactly what
`.github/workflows/ci.yml` runs — a red build is reproducible locally with the
same command. In order:

```bash
npm run check:fmt       # cargo fmt --check
npm run check:clippy    # clippy, all targets and features, warnings are errors
npm run check:rust      # cargo test --locked
npm run check:bindings  # regenerate ts-rs bindings; the tree must stay clean
npm run typecheck       # tsc --noEmit
npm test                # vitest
npm run build           # production bundle
```

**Layout.** Tests live beside the module they cover (`src/lib/path.test.ts`,
`src/store/panesStore.test.ts`, …). `src/test/` holds only the shared
machinery: `setup.ts` (localStorage shim plus the reset of DOM, timers, mocks,
Tauri harness and Zustand stores between tests), `tauri.ts` (the typed IPC
stand-ins) and `fixtures.ts`.

**Choosing a mode.** The IPC client behaves differently with and without a
Tauri runtime, and both are real. A test says which it wants:

```ts
import { useDemo, useTauri } from "@/test/tauri";

useDemo();                                   // browser build: demo fallbacks
const tauri = useTauri({ commands: { … } }); // mocked runtime + event pushes
```

`useDemo()` is the default, so the frontend suite needs no native process —
which is what lets it run in CI. An unmocked command fails loudly rather than
returning `undefined`.

**Live tests.** The protocol tests (SFTP, FTP/FTPS, SMB, WebDAV, the SSH
terminal, the connection timeout) and the OS-keychain tests are `#[ignore]`d:
they need real servers and real credentials, so they never run in pull-request
CI. The Docker command for each server is in the doc comment above its test.

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib live_ \
  -- --ignored --nocapture --test-threads=1
```

`.github/workflows/live-tests.yml` is the manual counterpart; dispatched
without those servers it stops with the setup instructions rather than
pretending to pass.

**CI jobs.** `check` (the full gate, Linux), `rust` (Ubuntu/macOS/Windows, so
the `cfg`-gated platform code keeps compiling and its portable tests keep
passing) and `msrv` (Rust 1.85 with `--locked`). Workflow permissions are
`contents: read`, and no job receives secrets.
