// ============================================================
// Zustand für laufende Kopier-/Verschiebe-Vorgänge.
// Jeder Transfer hat zwei Fortschrittsebenen (Datei + Gesamt),
// kann minimiert (in die Statusleiste) und wiederhergestellt werden.
// ============================================================

import { create } from "zustand";
import type { OpDone, OpProgress } from "@/ipc/bindings";

export type TransferOp = "copy" | "move" | "extract" | "pack";

export interface Transfer {
  id: string;
  op: TransferOp;
  fileName: string;
  fileDone: number;
  fileTotal: number;
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number;
  minimized: boolean;
  done: boolean;
  cancelled: boolean;
  /** Vom Backend gemeldet: Vorgang ist gerade pausiert. */
  paused: boolean;
  /** Wartet in der Kopier-Queue auf den Start (läuft noch nicht). */
  queued: boolean;
  errors: string[];
}

interface TransfersStore {
  transfers: Transfer[];

  start: (op: TransferOp, id: string) => void;
  apply: (p: OpProgress) => void;
  finish: (d: OpDone) => void;
  minimize: (id: string, value: boolean) => void;
  setPaused: (id: string, value: boolean) => void;
  setQueued: (id: string, value: boolean) => void;
  remove: (id: string) => void;
}

function emptyTransfer(id: string, op: TransferOp): Transfer {
  return {
    id,
    op,
    fileName: "",
    fileDone: 0,
    fileTotal: 0,
    filesDone: 0,
    filesTotal: 0,
    bytesDone: 0,
    bytesTotal: 0,
    minimized: false,
    done: false,
    cancelled: false,
    paused: false,
    queued: false,
    errors: [],
  };
}

export const useTransfers = create<TransfersStore>((set) => ({
  transfers: [],

  start: (op, id) =>
    set((s) => ({ transfers: [...s.transfers, emptyTransfer(id, op)] })),

  apply: (p) =>
    set((s) => ({
      transfers: s.transfers.map((t) =>
        t.id === p.id
          ? {
              ...t,
              op: p.op as TransferOp,
              fileName: p.file_name,
              fileDone: p.file_done,
              fileTotal: p.file_total,
              filesDone: p.files_done,
              filesTotal: p.files_total,
              bytesDone: p.bytes_done,
              bytesTotal: p.bytes_total,
              paused: p.paused,
              queued: false,
            }
          : t,
      ),
    })),

  finish: (d) =>
    set((s) => ({
      transfers: s.transfers.map((t) =>
        t.id === d.id
          ? {
              ...t,
              done: true,
              cancelled: d.cancelled,
              paused: false,
              queued: false,
              errors: d.errors,
            }
          : t,
      ),
    })),

  minimize: (id, value) =>
    set((s) => ({
      transfers: s.transfers.map((t) =>
        t.id === id ? { ...t, minimized: value } : t,
      ),
    })),

  setPaused: (id, value) =>
    set((s) => ({
      transfers: s.transfers.map((t) =>
        t.id === id ? { ...t, paused: value } : t,
      ),
    })),

  setQueued: (id, value) =>
    set((s) => ({
      transfers: s.transfers.map((t) =>
        t.id === id ? { ...t, queued: value } : t,
      ),
    })),

  remove: (id) =>
    set((s) => ({ transfers: s.transfers.filter((t) => t.id !== id) })),
}));
