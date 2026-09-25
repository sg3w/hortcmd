import { hasTauri, invoke } from "../invoke";
import type { RecoveryEntry } from "../bindings/RecoveryEntry";
import type { RecoveryLaunch } from "../bindings/RecoveryLaunch";
import type { RecoverySettings } from "../bindings/RecoverySettings";

// Serialize intent registration in user order, independently of backend worker scheduling.
let pending: Promise<void> = Promise.resolve();
export function queueRecovery(id: string, kind: "copy" | "move", sources: string[], destination: string, settings: RecoverySettings, review?: string): Promise<void> {
  if (!hasTauri()) return Promise.resolve();
  const registered = pending.then(() => invoke<void>("recovery_queue_transfer", { id, kind, sources, destination, settings, ...(review ? { review } : {}) }));
  pending = registered.catch(() => {});
  return registered;
}
export const listRecovery = (): Promise<RecoveryEntry[]> => hasTauri() ? invoke("recovery_list") : Promise.resolve([]);
export const prepareRecovery = (id: string, resume: boolean): Promise<RecoveryLaunch> => invoke("recovery_prepare", { id, resume });
export const cleanupRecovery = (id: string): Promise<void> => invoke("recovery_cleanup", { id });
export const discardRecovery = (id: string): Promise<void> => hasTauri() ? invoke("recovery_discard", { id }) : Promise.resolve();
export const acknowledgeRecovery = (id: string): Promise<void> => hasTauri() ? invoke("recovery_acknowledge", { id }) : Promise.resolve();
export const recoveryDestination = (id: string): Promise<string> => invoke("recovery_destination", { id });
