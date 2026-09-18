import { useEffect } from "react";
import { hasTauri } from "@/ipc/invoke";
import { usePanes } from "@/store/panesStore";
import { useSettings } from "@/store/settingsStore";
import { publishContext } from "./context";
export default function PluginRuntime() {
  useEffect(() => {
    if (!hasTauri) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const publish = () => { void publishContext().catch(() => {}); };
    const schedule = () => { clearTimeout(timer); timer = setTimeout(publish, 50); };
    publish(); const panes = usePanes.subscribe(schedule); const settings = useSettings.subscribe(schedule);
    return () => { clearTimeout(timer); panes(); settings(); };
  }, []);
  return null;
}
