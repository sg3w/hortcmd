import { hasTauri, invoke } from "../invoke";
import type { OperationPlan } from "../bindings/OperationPlan";
import type { RecoverySettings } from "../bindings/RecoverySettings";
export interface PreflightRequest { kind: "copy" | "move"; sources: string[]; destination: string; settings: RecoverySettings }
export const transferPreflight = (token: string, request: PreflightRequest): Promise<OperationPlan | null> => hasTauri ? invoke("transfer_preflight", { token, ...request }) : Promise.resolve(null);
export const cancelPreflight = (token: string): Promise<void> => hasTauri ? invoke("cancel_preflight", { token }) : Promise.resolve();
