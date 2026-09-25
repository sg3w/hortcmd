import { useEffect, useState } from "react";
import { hasTauri, homeDir, mcpAudit, mcpClearAudit, mcpSetConfig, mcpStatus, mcpToken } from "@/ipc/client";
import type { McpConfig } from "@/ipc/bindings/McpConfig";
import type { McpAuditEntry } from "@/ipc/bindings/McpAuditEntry";
import { useT } from "@/store/settingsStore";
import { smallBtn } from "@/components/ui/settings/controls";

export function McpForm() {
  const t = useT();
  const [config, setConfig] = useState<McpConfig | null>(null);
  const [endpoint, setEndpoint] = useState("");
  const [token, setToken] = useState("");
  const [error, setError] = useState("");
  const [audit, setAudit] = useState<McpAuditEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const failed = () => setError(t("mcp.error"));
  useEffect(() => {
    if (!hasTauri()) return;
    let disposed = false;
    void mcpStatus().then(s => { if (!disposed) { setConfig(s.config); setEndpoint(s.endpoint ?? ""); } }).catch(failed);
    void mcpAudit().then(a => { if (!disposed) setAudit(a); }).catch(failed);
    return () => { disposed = true; };
  }, []);
  if (!hasTauri()) return <p>{t("mcp.native")}</p>;
  if (!config) return <p role="status">{error || t("mcp.loading")}</p>;
  const patch = (change: Partial<McpConfig>) => setConfig({ ...config, ...change });
  const save = async () => {
    setBusy(true); setError("");
    try {
      setConfig(await mcpSetConfig(config));
      setEndpoint((await mcpStatus()).endpoint ?? "");
    } catch { failed(); } finally { setBusy(false); }
  };
  return <div className="space-y-3 text-sm">
    <p className="text-dim">{t("mcp.hint")}</p>
    {error && <p role="alert">{error}</p>}
    <p role="status">{endpoint || t("mcp.off")}</p>
    {([
      ["enabled", "mcp.enabled"], ["allowRead", "mcp.read"], ["allowWrite", "mcp.write"], ["allowDelete", "mcp.delete"],
      ["allowPermanentDelete", "mcp.permanent"], ["confirmWrites", "mcp.confirm"], ["rootsFollowPanes", "mcp.panes"],
      ["rootsFollowRemotes", "mcp.remotes"],
    ] as const).map(([key, label]) => <label className="flex gap-2" key={key}>
      <input type="checkbox" checked={config[key]} onChange={e => patch({ [key]: e.target.checked })} />{t(label)}
    </label>)}
    <label className="block">{t("mcp.port")}<input className="ml-2 bg-panel border border-edge" type="number" min={0} max={65535} value={config.port} onChange={e => patch({ port: Number(e.target.value) })} /></label>
    <label className="block">{t("mcp.limit")}<input className="ml-2 bg-panel border border-edge" type="number" min={1024} max={67108864} value={config.maxReadBytes} onChange={e => patch({ maxReadBytes: Number(e.target.value) })} /></label>
    <label className="block">{t("mcp.roots")}<textarea className="block w-full bg-panel border border-edge" rows={4} value={config.roots.join("\n")} onChange={e => patch({ roots: e.target.value.split("\n") })} /></label>
    <div className="flex flex-wrap gap-2">
      <button className={smallBtn} onClick={() => void homeDir().then(path => patch({ roots: [...config.roots, path] })).catch(failed)}>{t("mcp.home")}</button>
      <button disabled={busy} className={smallBtn} onClick={() => void save()}>{t("mcp.save")}</button>
      <button className={smallBtn} onClick={() => void mcpToken().then(setToken).catch(failed)}>{t("mcp.token")}</button>
      <button className={smallBtn} onClick={() => void mcpToken(true).then(setToken).catch(failed)}>{t("mcp.rotate")}</button>
    </div>
    {token && <label className="block">{t("mcp.token")}<input className="block w-full bg-panel border border-edge font-mono" readOnly value={token} onFocus={e => e.target.select()} /></label>}
    <div className="flex gap-2">
      <button className={smallBtn} onClick={() => void mcpAudit().then(setAudit).catch(failed)}>{t("mcp.audit")}</button>
      <button className={smallBtn} onClick={() => void mcpClearAudit().then(() => setAudit([])).catch(failed)}>{t("mcp.clear")}</button>
    </div>
    <ul className="max-h-48 overflow-auto text-xs">{audit.map((entry, i) => <li key={i}>{new Date(entry.at * 1000).toLocaleTimeString()} · {entry.client} · {entry.tool} · {entry.outcome} · {entry.target} {entry.detail}</li>)}</ul>
  </div>;
}
