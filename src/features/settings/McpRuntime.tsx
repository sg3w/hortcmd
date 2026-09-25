import { useEffect, useState } from "react";
import { hasTauri, mcpAnswer, mcpPublishPanes, mcpStatus, onMcpConfirm, onMcpConfirmClosed } from "@/ipc/client";
import type { McpConfirmRequest } from "@/ipc/bindings/McpConfirmRequest";
import { panelOf, usePanes } from "@/store/panesStore";
import { useT } from "@/store/settingsStore";
import { AppDialog } from "@/components/ui/AppDialog";
import { smallBtn } from "@/components/ui/settings/controls";

export function McpRuntime() {
  const t = useT();
  const [requests, setRequests] = useState<McpConfirmRequest[]>([]);
  const [remember, setRemember] = useState(false);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState(false);
  const [answering, setAnswering] = useState(false);
  const request = requests[0];
  useEffect(() => {
    if (!hasTauri()) return;
    let disposed = false;
    const listening = Promise.all([
      onMcpConfirm(r => { if (!disposed) setRequests(q => [...q, r]); }),
      onMcpConfirmClosed(id => { if (!disposed) setRequests(q => q.filter(r => r.id !== id)); }),
    ].map(subscription => subscription.catch(() => { if (!disposed) setError(true); return () => {}; })));
    const update = () => void mcpStatus().then(s => { if (!disposed) setRunning(s.running); }).catch(() => {});
    update();
    const timer = window.setInterval(update, 5000);
    return () => { disposed = true; clearInterval(timer); void listening.then(unlisteners => unlisteners.forEach(unlisten => unlisten())); };
  }, []);
  useEffect(() => {
    if (!running) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const publish = () => {
      const state = usePanes.getState();
      const pane = (side: "left" | "right") => {
        const p = panelOf(state, side);
        return { path: p.archive ?? p.path, selection: [...p.selected], sort: `${p.sort.key}:${p.sort.asc ? "asc" : "desc"}`, filter: p.filter };
      };
      void mcpPublishPanes({ active: state.active, left: pane("left"), right: pane("right") }).catch(() => {});
    };
    publish();
    const unsubscribe = usePanes.subscribe(() => { clearTimeout(timer); timer = setTimeout(publish, 80); });
    return () => { clearTimeout(timer); unsubscribe(); };
  }, [running]);
  useEffect(() => { setRemember(false); setError(false); }, [request?.id]);
  const answer = async (approved: boolean) => {
    if (!request || answering) return;
    setAnswering(true);
    try {
      await mcpAnswer(request.id, approved, approved && remember);
      setRequests(q => q.filter(r => r.id !== request.id)); setRemember(false); setError(false);
    } catch { setError(true); } finally { setAnswering(false); }
  };
  return <>
    {running && <span className="bg-accent-dim px-2 text-xs" role="status">{t("mcp.running")}</span>}
    <AppDialog dialogKey="mcp-confirm" open={!!request} onClose={() => void answer(false)} titleBar={t("mcp.request")} defaultSize={{ w: 520, h: 260 }} minSize={{ w: 380, h: 220 }}>
      <div className="space-y-3 p-4 text-sm">
        <p>{request?.client} · {request?.tool}</p>
        <p className="break-all">{request?.summary}</p>
        <label className="flex gap-2"><input type="checkbox" checked={remember} onChange={e => setRemember(e.target.checked)} />{t("mcp.remember")}</label>
        {error && <p role="alert">{t("mcp.error")}</p>}
        <div className="flex gap-2"><button disabled={answering} className={smallBtn} onClick={() => void answer(false)}>{t("mcp.decline")}</button><button disabled={answering} className={smallBtn} onClick={() => void answer(true)}>{t("mcp.approve")}</button></div>
      </div>
    </AppDialog>
  </>;
}
