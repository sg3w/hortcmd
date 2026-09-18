import { useEffect, useState, type ReactNode } from "react";
import { pluginClose, pluginPreview } from "@/ipc/domains/plugins";
import type { PluginInstance } from "@/ipc/bindings/PluginInstance";
import { usePanes, type Side } from "@/store/panesStore";
import { PluginFrame } from "./PluginFrame";
import { publishContext } from "./context";
export default function PluginPreview({path, side, children, enabled = true}: {path?: string; side?: Side; children: ReactNode; enabled?: boolean}) {
  const [instance,setInstance] = useState<PluginInstance|null>(null);
  useEffect(() => {
    setInstance(null); if (!path || !enabled) return;
    let disposed = false; let current: PluginInstance | null = null;
    void publishContext().then(() => pluginPreview(path,side ?? usePanes.getState().active)).then(info => {
      current=info;
      if (disposed) { if(info) void pluginClose(info.id).catch(() => {}); }
      else setInstance(info);
    }).catch(() => {});
    return () => { disposed=true; if(current) void pluginClose(current.id).catch(() => {}); };
  }, [path,side,enabled]);
  return instance ? <><p className="px-2 text-xs text-dim">{instance.title}</p><PluginFrame id={instance.id} title={instance.title} onTitle={title => setInstance(current => current ? {...current,title} : null)} onFailure={() => setInstance(null)} /></> : <>{children}</>;
}
