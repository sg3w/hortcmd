/** Author-facing SDK types. The host injects window.hortcmd into each frame. */
import type { PluginRequest } from "@/ipc/bindings/PluginRequest";
import type { PluginEvent } from "@/ipc/bindings/PluginEvent";
export interface PluginApi {
  request(request: PluginRequest): Promise<unknown>;
  onEvent(listener: (event: PluginEvent) => void): () => void;
}
declare global { interface Window { hortcmd?: PluginApi } }
