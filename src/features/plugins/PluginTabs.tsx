import { useState } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { pluginOpen } from "@/ipc/domains/plugins";
import { usePanes, type Side } from "@/store/panesStore";
import { useT } from "@/store/settingsStore";
import { usePlugins } from "./pluginsStore";
import { PluginIcon } from "./PluginIcon";
import { publishContext } from "./context";
import { describeErrorFull } from "@/store/errorText";
export default function PluginTabs({side}: {side: Side}) {
  const state = usePlugins(); const t = useT(); const [error,setError] = useState<string|null>(null);
  const tabs = state.tabs.filter(tab => tab.side === side);
  const contributions = state.records.filter(r => r.active && r.granted.includes("ui:tab") && !r.error).flatMap(r => r.manifest!.contributes.tabs.map(view => ({plugin:r.id, name:r.manifest!.name, view})));
  const open = async (plugin: string, id: string) => {
    setError(null);
    const item = contributions.find(c => c.plugin === plugin && c.view.id === id); if (!item) return;
    try {
      await publishContext();
      const instance = await pluginOpen(plugin,id,false,side);
      usePlugins.getState().addView({instance,side,contribution:item.view,title:item.view.title});
      usePanes.getState().setActive(usePlugins.getState().tabs.find(t => t.instance.id === instance.id)?.side ?? side);
    } catch(error) { setError(describeErrorFull(error)); }
  };
  const activate = (id: string) => { state.activateView(side,id); usePanes.getState().setActive(side); };
  return <>
    {tabs.length > 0 && <div role="tablist" aria-label={t("plugins.tabs")} aria-owns={tabs.map(tab => `${side}-plugin-${tab.instance.id}`).join(" ")} />}
    {tabs.map((tab,index) => <div key={tab.instance.id} className="flex shrink-0 items-center gap-1 rounded-t border border-edge px-2 text-xs">
      <button id={`${side}-plugin-${tab.instance.id}`} role="tab" aria-selected={state.activeView[side] === tab.instance.id}
        tabIndex={state.activeView[side] === tab.instance.id ? 0 : -1} aria-controls={`${side}-plugin-view-${tab.instance.id}`}
        className="flex items-center gap-1" onClick={() => activate(tab.instance.id)}
        onKeyDown={event => {
          let next = index;
          if(event.key === "ArrowRight") next=(index+1)%tabs.length;
          else if(event.key === "ArrowLeft") next=(index+tabs.length-1)%tabs.length;
          else if(event.key === "Home") next=0;
          else if(event.key === "End") next=tabs.length-1;
          else if(event.key === "Delete" && tab.contribution.closable) { event.preventDefault(); state.closeView(tab.instance.id); return; }
          else return;
          event.preventDefault(); activate(tabs[next].instance.id); document.getElementById(`${side}-plugin-${tabs[next].instance.id}`)?.focus();
        }}><PluginIcon icon={tab.contribution.icon}/>{tab.title}</button>
      {tab.contribution.closable && <button aria-label={`${t("tab.close")}: ${tab.title}`} onClick={() => state.closeView(tab.instance.id)}>×</button>}
    </div>)}
    {contributions.length > 0 && <DropdownMenu.Root><DropdownMenu.Trigger className="shrink-0 px-2 text-xs text-dim">{t("plugins.tabs")}</DropdownMenu.Trigger>
      <DropdownMenu.Portal><DropdownMenu.Content className="z-50 rounded border border-edge bg-panel p-1 text-sm text-text shadow-lg">
        {contributions.map(({plugin,name,view}) => <DropdownMenu.Item key={`${plugin}:${view.id}`} className="cursor-default rounded px-2 py-1 outline-none focus:bg-accent-dim" onSelect={() => void open(plugin,view.id)}>{name}: {view.title}</DropdownMenu.Item>)}
      </DropdownMenu.Content></DropdownMenu.Portal>
    </DropdownMenu.Root>}
    {error && <span role="alert" className="text-xs">{error}</span>}
  </>;
}
