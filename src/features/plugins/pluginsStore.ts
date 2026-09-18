import { create } from "zustand";
import { listPlugins } from "@/ipc/domains/plugins";
import type { PluginRecord } from "@/ipc/bindings/PluginRecord";
let generation = 0;
interface PluginsState {
  records: PluginRecord[];
  error: string | null;
  refresh: () => Promise<void>;
}
export const usePlugins = create<PluginsState>((set) => ({
  records: [], error: null,
  refresh: async () => {
    const current = ++generation;
    try {
      const records = await listPlugins();
      if (current === generation) set({records, error:null});
    } catch (error) { if (current === generation) set({records:[], error:String(error)}); }
  },
}));
