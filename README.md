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

## Requirements

- **Rust** (stable): https://rustup.rs → `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js** ≥ 18 (installed: v26)
- Tauri CLI: `npm run tauri` (via devDependency)

## Development

```bash
npm install
npm run dev            # frontend only in the browser (demo data, no Rust)
npm run tauri dev      # native app: Vite + Rust backend
```

## Scripts

```bash
npm run typecheck      # tsc --noEmit
npm run build          # frontend production build
npm run gen:types      # ts-rs: Rust structs → src/ipc/bindings/*.ts
npm run tauri build    # build native app
```

> Without a running Tauri runtime (pure browser) the IPC client automatically
> uses demo data – handy for layout work without a Rust build.
