# CLAUDE.md

# hortcmd

hortcmd ist ein plattformübergreifender Dateimanager im Stil von Total Commander, entwickelt mit **Rust**, **Tauri v2**, **React** und **TypeScript**.

## Grundsätze

- Bevorzuge **Rust** für alle Dateisystem-, System- und Backend-Funktionen.
- Das Frontend dient ausschließlich der Darstellung und Benutzerinteraktion.
- Kommunikation zwischen Frontend und Backend erfolgt ausschließlich über **Tauri Commands**.
- Bestehende Architektur und Projektstruktur beibehalten.
- Änderungen sollen klein, nachvollziehbar und wartbar sein.

## Architektur

```
src/          React + TypeScript
src-tauri/    Rust + Tauri
```

Geschäftslogik gehört ins Backend. Das Frontend enthält keine duplizierte Dateisystemlogik.

## Backend

- Robuster und idiomatischer Rust-Code
- Fehler über `Result` behandeln, keine unnötigen `panic!`
- Plattformunterschiede (Windows, macOS, Linux) berücksichtigen
- Komplexe Logik in Services auslagern, Commands möglichst schlank halten
- IPC-Datentypen über **ts-rs** exportieren

## Frontend

- React Functional Components
- TypeScript strikt verwenden
- Zustand für State Management
- TanStack Table und Virtual für große Verzeichnisse
- Radix UI für Dialoge und Menüs
- Tailwind CSS für Styling

## Codequalität

Bevorzuge:

- kleine, gut strukturierte Funktionen
- sprechende Namen
- möglichst wenig Code-Duplizierung
- saubere Fehlerbehandlung
- Performance ohne unnötige Optimierung

Vermeide:

- unnötige Abstraktionen
- große Dateien mit vielen Verantwortlichkeiten
- Logik im Frontend, die ins Backend gehört

## Entwicklung

Nach Änderungen prüfen:

```bash
cargo check
npm run typecheck
npm run build
```

Bei Änderungen an IPC-Typen zusätzlich:

```bash
npm run gen:types
```

## Prioritäten

1. Stabilität
2. Datensicherheit
3. Wartbarkeit
4. Performance
5. Plattformübergreifende Kompatibilität