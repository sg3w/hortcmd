import { usePlugins } from "./pluginsStore";
import { PluginFrame } from "./PluginFrame";
import type { Side } from "@/store/panesStore";
export default function PluginPaneView({side}: {side: Side}) {
  const state = usePlugins(); const tabs = state.tabs.filter(t => t.side === side);
  return <>{tabs.map(tab => <div role="tabpanel" id={`${side}-plugin-view-${tab.instance.id}`} hidden={state.activeView[side] !== tab.instance.id} aria-labelledby={`${side}-plugin-${tab.instance.id}`} className={state.activeView[side] === tab.instance.id ? "flex min-h-0 flex-1 flex-col" : "hidden"} key={tab.instance.id}>
    <PluginFrame id={tab.instance.id} title={tab.title} onTitle={title => state.titleView(tab.instance.id,title)} onFailure={() => state.closeView(tab.instance.id)} />
  </div>)}</>;
}
