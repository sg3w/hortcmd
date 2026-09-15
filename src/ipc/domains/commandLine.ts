import { invoke } from "../invoke";
import type { CommandOutput } from "../bindings/CommandOutput";

export const commandStart = (path: string, command: string) => invoke<number>("command_start", { path, command });
export const commandStatus = (id: number) => invoke<CommandOutput>("command_status", { id });
export const commandCancel = (id: number) => invoke<void>("command_cancel", { id });
