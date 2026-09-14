// ============================================================
// The IPC surface (TICKET-035).
//
// One import path for every caller — `@/ipc/client` — behind which the
// implementation is split by domain. Callers were not changed by that
// split and need not know where a command lives; this file is what
// keeps that true.
//
// The layout behind it:
//
//   invoke.ts      the transport, and the one place that knows whether
//                  a Tauri runtime is present
//   events.ts      event routing: one listener set, fanned out to
//                  subscribers, fed by the backend *and* by the demo
//   demo.ts        every browser-demo answer, typed against the same
//                  generated bindings as the real ones
//   types.ts       the few IPC types the frontend owns
//   domains/*.ts   the commands, grouped by what they are about
//
// Nothing here adds behaviour: a re-export that owned logic would be
// the pass-through layer this split exists to avoid.
// ============================================================

export { hasTauri } from "./invoke";
export type { SearchMode, SearchOptions, TransferOp } from "./types";

export * from "./events";
export * from "./domains/caps";
export * from "./domains/compare";
export * from "./domains/fs";
export * from "./domains/net";
export * from "./domains/props";
export * from "./domains/search";
export * from "./domains/transfers";

export * from "./domains/mcp";

export * from "./domains/refresh";
export * from "./domains/window";

export * from "./domains/recovery";

export * from "./domains/undo";
export * from "./domains/index";
export { normalizeLocation } from "./domains/location";
export { fileIcons } from "./domains/icons";
