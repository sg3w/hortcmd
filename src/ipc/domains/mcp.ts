import { hasTauri, invoke } from "../invoke";
import type { McpConfig } from "../bindings/McpConfig";
import type { McpStatus } from "../bindings/McpStatus";
import type { McpPanes } from "../bindings/McpPanes";
import type { McpAuditEntry } from "../bindings/McpAuditEntry";
import type { McpConfirmRequest } from "../bindings/McpConfirmRequest";

export const mcpStatus = () => invoke<McpStatus>("mcp_status");
export const mcpSetConfig = (config: McpConfig) => invoke<McpConfig>("mcp_set_config", { config });
export const mcpToken = (regenerate = false) => invoke<string>(regenerate ? "mcp_regenerate_token" : "mcp_token");
export const mcpAudit = () => invoke<McpAuditEntry[]>("mcp_audit");
export const mcpClearAudit = () => invoke<void>("mcp_clear_audit");
export const mcpPublishPanes = (panes: McpPanes) => invoke<void>("mcp_publish_panes", { panes });
export const mcpAnswer = (id: string, approved: boolean, remember: boolean) => invoke<void>("mcp_answer", { id, approved, remember });

let eventModule: Promise<typeof import("@tauri-apps/api/event")> | undefined;
const events = () => eventModule ??= import("@tauri-apps/api/event");

export async function onMcpConfirm(callback: (request: McpConfirmRequest) => void) {
  if (!hasTauri) return () => {};
  const { listen } = await events();
  return listen<McpConfirmRequest>("mcp-confirm", (e) => callback(e.payload));
}

export async function onMcpConfirmClosed(callback: (id: string) => void) {
  if (!hasTauri) return () => {};
  const { listen } = await events();
  return listen<string>("mcp-confirm-closed", (e) => callback(e.payload));
}
