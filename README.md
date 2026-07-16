# hortcmd

Ein Dateicommander im Stil von Total Commander, umgesetzt mit **Rust** + **Tauri v2**.
Zwei-Fenster-Layout (Dual-Pane), Funktionstastenleiste (F3–F8), Laufwerks- und Pfadauswahl.

Architektur- und Entwurfsdetails: siehe [Concept.md](Concept.md).

## Tech-Stack

| Schicht        | Technologie                                        |
|----------------|----------------------------------------------------|
| Backend        | Rust (Tauri v2 Commands, `std::fs`, `sysinfo`)     |
| Sprache FE     | TypeScript + React                                 |
| Tabelle        | TanStack Table (headless) + TanStack Virtual       |
| State          | Zustand                                            |
| Overlays/Menüs | Radix UI (shadcn-Stil) – Kontextmenü, Select        |
| Icons          | Lucide (`lucide-react`)                            |
| Styling        | Tailwind CSS                                        |
| Typen-Sync     | ts-rs (Rust-Structs → TS-Typen)                    |
| Bundler        | Vite                                               |

## Projektstruktur

```
hortcmd/
├── index.html                  # Einstiegspunkt (lädt src/main.tsx)
├── package.json                # Skripte & Abhängigkeiten
├── vite.config.ts · tsconfig*  # Build-/TS-Konfiguration
├── tailwind.config.js · postcss.config.js
├── src/                        # Frontend (React + TS)
│   ├── main.tsx · App.tsx      # Bootstrap + Layout-Shell
│   ├── index.css               # Tailwind + TC-Theme-Tokens
│   ├── ipc/                    # invoke-Client + ts-rs-Typen (bindings/)
│   ├── store/                  # Zustand-Store (panesStore.ts)
│   ├── features/panel/         # Panel, FileTable, Spalten, Drive/Path/Status
│   ├── features/commander/     # Tastatur, Auswahl, Aktionen, F-Leiste
│   ├── components/ui/          # Radix-Primitive (Kontextmenü …)
│   └── lib/                    # format, path, cn
└── src-tauri/                  # Rust-Backend
    ├── Cargo.toml · build.rs · tauri.conf.json
    ├── capabilities/ · icons/
    └── src/
        ├── main.rs · lib.rs    # Bin-Einstieg + Command-Registrierung
        └── commands/fs.rs      # list_dir, list_drives, home_dir (+ ts-rs)
```

## Voraussetzungen

- **Rust** (stable): https://rustup.rs → `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js** ≥ 18 (vorhanden: v26)
- Tauri-CLI: `npm run tauri` (über devDependency)

## Entwicklung

```bash
npm install
npm run dev            # nur Frontend im Browser (Demo-Daten, ohne Rust)
npm run tauri dev      # native App: Vite + Rust-Backend
```

## Skripte

```bash
npm run typecheck      # tsc --noEmit
npm run build          # Frontend-Produktions-Build
npm run gen:types      # ts-rs: Rust-Structs → src/ipc/bindings/*.ts
npm run tauri build    # native App bauen
```

> Ohne laufende Tauri-Runtime (reiner Browser) nutzt der IPC-Client automatisch
> Demo-Daten – praktisch für Layout-Arbeit ohne Rust-Build.

