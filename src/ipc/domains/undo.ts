import { hasTauri, invoke } from "../invoke";
import type { UndoEntry } from "../bindings/UndoEntry";
export const previewUndo = (): Promise<UndoEntry[]> => hasTauri() ? invoke("undo_preview") : Promise.resolve([]);
export const executeUndo = (id: string): Promise<UndoEntry[]> => invoke("undo_execute", { id });
import type { UndoClassification } from "../bindings/UndoClassification";
export interface UndoRequest { kind: string; sources: string[]; target: string; directory: boolean }
export const classifyUndo = (request: UndoRequest): Promise<UndoClassification> => hasTauri() ? invoke("undo_classify", { ...request }) : Promise.resolve("irreversible");

export const forgetUndo = (id: string): Promise<UndoEntry[]> => invoke("undo_forget", { id });

/** Pushes the beta setting to the backend, which holds its own copy (TICKET-391). */
export const setUndoEnabled = (enabled: boolean): Promise<void> =>
  hasTauri() ? invoke("undo_set_enabled", { enabled }) : Promise.resolve();
