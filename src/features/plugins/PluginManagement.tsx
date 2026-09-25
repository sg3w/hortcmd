import { useEffect, useState } from "react";
import { AppDialog } from "@/components/ui/AppDialog";
import { pluginActivate, pluginDeactivate, pluginInstall } from "@/ipc/domains/plugins";
import { hasTauri } from "@/ipc/invoke";
import type { PluginRecord } from "@/ipc/bindings/PluginRecord";
import { useT } from "@/store/settingsStore";
import { PluginIcon } from "./PluginIcon";
import { usePlugins } from "./pluginsStore";
import { describeErrorFull } from "@/store/errorText";
const permissionKeys = {
  "read:context":"plugins.perm.context", "read:fs":"plugins.perm.read",
  "fs:write":"plugins.perm.write", "op:copy":"plugins.perm.copy",
  "op:limit":"plugins.perm.limit", "events:selection":"plugins.perm.selection",
  "events:transfer":"plugins.perm.transfer", "ui:window":"plugins.perm.window",
  "ui:commandbar":"plugins.perm.commandbar", "ui:preview":"plugins.perm.preview",
  "ui:tab":"plugins.perm.tab", "notify":"plugins.perm.notify",
} as const;
const button="rounded border border-edge px-2 py-1 text-sm hover:bg-accent-dim disabled:opacity-50";
export function PluginManagement() {
  const t=useT();const {records,refresh,error:registryError}=usePlugins();
  const [source,setSource]=useState("");const [busy,setBusy]=useState(false);
  const [error,setError]=useState<string|null>(null);
  const [grant,setGrant]=useState<PluginRecord|null>(null);
  const [remove,setRemove]=useState<PluginRecord|null>(null);
  useEffect(()=>{void refresh();},[refresh]);
  const run=async(action:()=>Promise<unknown>)=> {
    setBusy(true);setError(null);
    try {await action();} catch(cause) {setError(describeErrorFull(cause));}
    finally {await refresh();setBusy(false);}
  };
  if(!hasTauri()) return <p>{t("plugins.desktop")}</p>;
  return <div className="space-y-4 text-sm">
    <p>{t("plugins.installHint")}</p>
    <label className="flex flex-col gap-1">{t("plugins.source")}
      <input className="rounded border border-edge bg-panel px-2 py-1" value={source} onChange={e=>setSource(e.target.value)} disabled={busy}/>
    </label>
    <div className="flex gap-2">
      <button className={button} disabled={busy || !source.trim()} onClick={()=>void run(()=>pluginInstall(source.trim()))}>{t("plugins.install")}</button>
      <button className={button} disabled={busy} onClick={()=>void run(refresh)}>{t("plugins.refresh")}</button>
    </div>
    {(error || registryError) && <p role="alert">{error || registryError}</p>}
    {!records.length && <p>{t("plugins.empty")}</p>}
    {records.map(record=><section key={record.id} className="space-y-2 rounded border border-edge p-3" aria-label={record.manifest?.name ?? record.id}>
      <div className="flex items-center gap-2">{record.manifest && <PluginIcon icon={record.manifest.icon}/>}<strong>{record.manifest?.name ?? record.id}</strong>
        <span>{record.manifest?.version}</span><span>{record.manifest?.runtime}</span>
        <span>{t(record.error ? "plugins.invalid" : record.active ? "plugins.active" : "plugins.inactive")}</span>
      </div>
      <p>{record.manifest?.description}</p><p className="text-dim">{record.id}{record.manifest && ` · ${record.manifest.author}`}</p>
      {record.error && <p role="alert">{record.error}</p>}
      <div className="flex flex-wrap gap-2">
        {record.active ? <button className={button} disabled={busy} onClick={()=>void run(()=>pluginDeactivate(record.id))}>{t("plugins.deactivate")}</button>
          : <button className={button} disabled={busy || !!record.error || !record.fingerprint} onClick={()=>setGrant(record)}>{t("plugins.activate")}</button>}
        <button className={button} disabled={busy || !source.trim() || !record.manifest} onClick={()=>void run(()=>pluginInstall(source.trim(),record.id))}>{t("plugins.update")}</button>
        <button className={button} disabled={busy} onClick={()=>setRemove(record)}>{t("plugins.uninstall")}</button>
      </div>
    </section>)}
    <AppDialog dialogKey="plugin-grants" open={grant!==null} onClose={()=>{if(!busy)setGrant(null);}} titleBar={t("plugins.grantTitle")} defaultSize={{w:540,h:420}}
      footer={<div className="flex justify-end gap-2 p-3"><button className={button} disabled={busy} onClick={()=>setGrant(null)}>{t("plugins.cancel")}</button><button className={button} disabled={busy} onClick={()=>{if(grant?.fingerprint)void run(async()=>{await pluginActivate(grant.id,grant.fingerprint!);setGrant(null);});}}>{t("plugins.approve")}</button></div>}>
      <div className="space-y-3 overflow-auto p-4"><strong>{grant?.manifest?.name}</strong><p>{t("plugins.grantHint")}</p>
        {grant?.manifest?.runtime === "native" && <p role="note" className="rounded border border-edge p-2">{t("plugins.nativeWarning")}</p>}
        <ul className="list-disc space-y-2 pl-5">{grant?.manifest?.permissions.map(permission=><li key={permission}><code>{permission}</code>{" — "}{t(permissionKeys[permission])}</li>)}</ul>
        {grant?.manifest?.permissions.length===0 && <p>{t("plugins.noPermissions")}</p>}
        {error && <p role="alert">{error}</p>}
      </div>
    </AppDialog>
    <AppDialog dialogKey="plugin-uninstall" open={remove!==null} onClose={()=>{if(!busy)setRemove(null);}} titleBar={t("plugins.uninstall")} defaultSize={{w:440,h:230}}
      footer={<div className="flex justify-end gap-2 p-3"><button className={button} disabled={busy} onClick={()=>setRemove(null)}>{t("plugins.cancel")}</button><button className={button} disabled={busy} onClick={()=>{if(remove)void run(async()=>{await pluginDeactivate(remove.id,true);setRemove(null);});}}>{t("plugins.uninstall")}</button></div>}>
      <div className="space-y-2 p-4"><p>{remove?.manifest?.name ?? remove?.id}</p><p>{t("plugins.removeHint")}</p>{error && <p role="alert">{error}</p>}</div>
    </AppDialog>
  </div>;
}
