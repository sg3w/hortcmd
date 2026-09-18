import { useEffect, useRef, useState } from "react";
import { pluginAssetUrl, pluginClose, pluginInfo, pluginPoll, pluginRequest } from "@/ipc/domains/plugins";
import type { PluginRequest } from "@/ipc/bindings/PluginRequest";
import { useT } from "@/store/settingsStore";

export function PluginFrame({id, title, onFailure, onTitle}: {
  id: string; title: string; onFailure?: () => void; onTitle?: (title: string) => void;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const lifetime = useRef({id, generation: 0});
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const callbacks = useRef({onFailure, onTitle}); callbacks.current = {onFailure, onTitle};
  const t = useT();
  useEffect(() => {
    const generation = lifetime.current.generation + 1;
    lifetime.current = {id, generation};
    const child = frame.current?.contentWindow;
    let live = true; let busy = 0; let polling = false;
    setReady(false); setFailed(false);
    const fail = () => {
      if (!live) return;
      live = false; setFailed(true); void pluginClose(id).catch(() => {}); callbacks.current.onFailure?.();
    };
    const timer = window.setTimeout(fail, 10000);
    const receive = async (event: MessageEvent) => {
      if (!live || !frame.current?.contentWindow || event.source !== frame.current.contentWindow || event.origin !== "null") return;
      const data = event.data as {channel?: string; id?: number; request?: PluginRequest} | null;
      if (!data || data.channel !== "hortcmd:request" || !Number.isSafeInteger(data.id) || !data.request || busy >= 32) return;
      try { if (JSON.stringify(data).length > 1024 * 1024) return; } catch { return; }
      busy++;
      try {
        const value = await pluginRequest(id, data.request);
        if (!live) return;
        frame.current?.contentWindow?.postMessage({channel:"hortcmd:response", id:data.id, value}, "*");
        if (data.request.type === "view:ready") { clearTimeout(timer); setReady(true); }
        if (data.request.type === "view:title") callbacks.current.onTitle?.(data.request.title);
        if (data.request.type === "view:requestClose") fail();
      } catch (error) {
        if (live) frame.current?.contentWindow?.postMessage({channel:"hortcmd:response", id:data.id, error:String(error)}, "*");
      } finally { busy--; }
    };
    window.addEventListener("message", receive);
    const interval = window.setInterval(async () => {
      if (!live || polling) return; polling = true;
      try {
        const events = await pluginPoll(id);
        if (live) for (const event of events) frame.current?.contentWindow?.postMessage({channel:"hortcmd:event", event}, "*");
      } catch { fail(); } finally { polling = false; }
    }, 150);
    return () => {
      child?.postMessage({channel:"hortcmd:event",event:{type:"view:close",instanceId:id}}, "*");
      live = false; clearTimeout(timer); clearInterval(interval); window.removeEventListener("message", receive);
      queueMicrotask(() => {
        if (lifetime.current.id !== id || lifetime.current.generation === generation) void pluginClose(id).catch(() => {});
      });
    };
  }, [id]);
  if (failed) return <p role="status">{t("plugins.failed")}</p>;
  return <div className="flex min-h-0 flex-1 flex-col">
    {!ready && <p role="status">{t("plugins.loading")}</p>}
    <iframe ref={frame} src={pluginAssetUrl(id)} title={title} sandbox="allow-scripts" referrerPolicy="no-referrer"
      onError={() => { setFailed(true); void pluginClose(id); callbacks.current.onFailure?.(); }}
      className="min-h-0 flex-1 border-0" />
  </div>;
}

export default function PluginWindow() {
  const id = new URLSearchParams(location.search).get("pluginHost") ?? "";
  const [title, setTitle] = useState("");
  useEffect(() => { void pluginInfo(id).then(info => setTitle(info.title)).catch(() => {}); }, [id]);
  useEffect(() => { if (title) document.title = title; }, [title]);
  return <main className="flex h-screen flex-col bg-panel text-text"><PluginFrame id={id} title={title} onTitle={setTitle} /></main>;
}
