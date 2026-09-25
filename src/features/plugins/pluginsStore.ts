import { create } from "zustand";
import { listPlugins, pluginClose } from "@/ipc/domains/plugins";
import type { PluginRecord } from "@/ipc/bindings/PluginRecord";
import type { PluginInstance } from "@/ipc/bindings/PluginInstance";
import type { PluginTabContribution } from "@/ipc/bindings/PluginTabContribution";
import type { Side } from "@/store/panesStore";
import { describeErrorFull } from "@/store/errorText";
export interface PluginTab {instance: PluginInstance; side: Side; contribution: PluginTabContribution; title: string}
let generation = 0;
interface PluginsState {
  records: PluginRecord[];
  tabs: PluginTab[];
  activeView: Record<Side, string | null>;
  activateView: (side: Side, id: string | null) => void;
  addView: (tab: PluginTab) => void;
  closeView: (id: string) => void;
  titleView: (id: string, title: string) => void;
  error: string | null;
  refresh: () => Promise<void>;
}
export const usePlugins = create<PluginsState>((set, get) => ({
  records: [], error: null, tabs:[], activeView:{left:null,right:null},
  activateView: (side, id) => set(s => ({activeView:{...s.activeView,[side]:id}})),
  addView: tab => set(s => {
    const existing = s.tabs.find(t => t.instance.id === tab.instance.id);
    return {tabs: existing ? s.tabs : [...s.tabs,tab], activeView:{...s.activeView,[existing?.side ?? tab.side]:tab.instance.id}};
  }),
  closeView: id => {
    void pluginClose(id).catch(() => {});
    set(s => ({tabs:s.tabs.filter(t => t.instance.id !== id), activeView:{left:s.activeView.left === id ? null : s.activeView.left, right:s.activeView.right === id ? null : s.activeView.right}}));
  },
  titleView: (id, title) => set(s => ({tabs:s.tabs.map(t => t.instance.id === id ? {...t,title} : t)})),
  refresh: async () => {
    const current = ++generation;
    try {
      const records = await listPlugins();
      if (current === generation) {
        set({records, error:null});
        for (const tab of get().tabs) if (!records.some(r => r.id === tab.instance.pluginId && r.active && r.granted.includes("ui:tab"))) get().closeView(tab.instance.id);
      }
    } catch (error) { if (current === generation) set({records:[], error:describeErrorFull(error)}); }
  },
}));
