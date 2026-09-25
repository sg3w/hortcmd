import type { DirListing } from "../bindings";
import type { RefreshBatch } from "../bindings/RefreshBatch";
import type { RefreshSubscription } from "../bindings/RefreshSubscription";
import { hasTauri, invoke } from "../invoke";

// Serialize replacement and teardown, including React StrictMode remounts.
let subscriptions = Promise.resolve();
export function refreshSubscribe(targets: RefreshSubscription[]): Promise<void> {
  const next = subscriptions.catch(() => {}).then(() => hasTauri()
    ? invoke<void>("refresh_subscribe", { subscriptions: targets }) : undefined);
  subscriptions = next;
  return next;
}
export async function refreshPoll(visible: boolean): Promise<RefreshBatch> {
  return hasTauri() ? invoke<RefreshBatch>("refresh_poll", { visible }) : {status:[],work:[]};
}
export async function refreshFinish(key: string, serial: number, error: string | null): Promise<void> {
  if (hasTauri()) await invoke<void>("refresh_finish", { key, serial, error });
}

export async function refreshRead(key: string, serial: number): Promise<DirListing> {
  return invoke<DirListing>("refresh_read", { key, serial });
}
