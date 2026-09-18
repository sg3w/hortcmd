import { useEffect, useState } from "react";
import { pluginLaunch } from "@/ipc/domains/plugins";
import { useT } from "@/store/settingsStore";
import { usePlugins } from "./pluginsStore";
import { PluginIcon } from "./PluginIcon";

export default function PluginCommandBar() {
  const records = usePlugins(s => s.records);
  const refresh = usePlugins(s => s.refresh);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const t = useT();
  useEffect(() => { void refresh(); }, [refresh]);
  const active = records.filter(p => p.active && !p.error && p.granted.includes("ui:commandbar") && p.manifest?.contributes.commandbar.length);
  if (!active.length) return null;
  const launch = async (plugin: string, button: string) => {
    setBusy(`${plugin}:${button}`); setError(null);
    try { await pluginLaunch(plugin, button); } catch (error) { setError(String(error)); } finally { setBusy(null); }
  };
  return <div className="shrink-0 border-b border-edge bg-header">
    <div role="group" aria-label={t("plugins.title")} className="flex items-center gap-2 overflow-x-auto px-2 py-1">
      {active.map(record => <div key={record.id} role="group" aria-label={record.manifest!.name} className="flex shrink-0 items-center gap-1 border-r border-edge pr-2">
        <span className="text-xs text-dim">{record.manifest!.name}</span>
        {record.manifest!.contributes.commandbar.map(button => <button key={button.id}
          title={`${record.manifest!.name}: ${button.title}`} disabled={busy !== null}
          onClick={() => void launch(record.id, button.id)}
          className="flex items-center gap-1 rounded px-2 py-1 text-xs text-text hover:bg-accent-dim focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent disabled:opacity-50">
          <PluginIcon icon={button.icon} /><span>{button.title}</span>
        </button>)}
      </div>)}
    </div>
    {error && <p role="alert" className="px-2 text-xs text-text">{error}</p>}
  </div>;
}
