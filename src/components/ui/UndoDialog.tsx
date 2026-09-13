import { useEffect, useState } from "react";
import { executeUndo, forgetUndo, previewUndo } from "@/ipc/client";
import type { UndoEntry } from "@/ipc/bindings/UndoEntry";
import type { TransKey } from "@/i18n/dictionaries";
import { useUndo } from "@/store/undoStore";
import { useT } from "@/store/settingsStore";
import { usePanes } from "@/store/panesStore";
import { describeErrorFull } from "@/store/errorText";
import { AppDialog } from "./AppDialog";
const status: Record<string, TransKey> = { interrupted: "undo.interrupted", recorded: "undo.recorded", ready: "undo.ready", changed: "undo.changed", collision: "undo.collision", expired: "undo.expired", pending: "undo.pending", applying: "undo.applying", failed: "undo.failed", restored: "undo.restored" };
const kinds: Record<string, TransKey> = { rename: "undo.rename", move: "undo.move", copy: "undo.copy", mkdir: "undo.mkdir", permissions: "undo.permissions", owner: "undo.owner", tags: "undo.tags" };
export function UndoDialog() {
  const { open, setOpen } = useUndo(); const t = useT();
  const [entries, setEntries] = useState<UndoEntry[]>([]);
  const [busy, setBusy] = useState(true); const [error, setError] = useState<string | null>(null);
  useEffect(() => { let live = true; void previewUndo().then(e => { if (live) setEntries(e); }).catch(e => { if (live) setError(describeErrorFull(e)); }).finally(() => { if (live) setBusy(false); }); return () => { live = false; }; }, []);
  const perform = async (id?: string) => {
    setBusy(true); setError(null);
    try { setEntries(id ? await executeUndo(id) : await previewUndo()); if (id) await Promise.all([usePanes.getState().refresh("left"), usePanes.getState().refresh("right")]); }
    catch (e) { setError(describeErrorFull(e)); }
    finally { setBusy(false); }
  };
  const forget = async (id: string) => {
    setBusy(true); setError(null);
    try { setEntries(await forgetUndo(id)); } catch (e) { setError(describeErrorFull(e)); } finally { setBusy(false); }
  };
  const next = entries.find(e => e.status === "ready")?.id;
  const button = "rounded border border-edge px-2 py-1 disabled:opacity-50";
  return <AppDialog dialogKey="fileUndo" open={open} onClose={() => { if (!busy) setOpen(false); }} titleBar={t("undo.title")} defaultSize={{ w: 700, h: 480 }} minSize={{ w: 360, h: 260 }}
    footer={<><button className={button} disabled={busy} onClick={() => void perform()}>{t("recovery.refresh")}</button><button className={button} disabled={busy} onClick={() => setOpen(false)}>{t("recovery.close")}</button></>}>
    <div className="min-h-0 flex-1 overflow-auto p-3" aria-busy={busy}>
      <p className="mb-3 text-sm">{t("undo.help")}</p>
      {error && <p role="alert">{error}</p>}
      {!busy && !entries.length && <p>{t("undo.empty")}</p>}
      <ol className="space-y-3">{entries.map(e => <li key={e.id} className="rounded border border-edge p-3">
        <h3 className="font-semibold">{t(kinds[e.kind] ?? "undo.title")}{e.id === next ? ` — ${t("undo.next")}` : ""}</h3>
        {e.group && <p>{t("undo.group").replace("{n}", String(entries.filter(r => r.group === e.group && r.status === "ready").length))}</p>}
        <p className="break-all">{e.path}{e.target ? ` → ${e.target}` : ""}</p>
        <p>{t(status[e.status] ?? "undo.failed")}</p>
        {e.retained && <p className="break-all">{t("undo.retained").replace("{path}", e.retained)}</p>}
        <button className={button} disabled={busy || e.status !== "ready"} onClick={() => void perform(e.id)}>{t("undo.apply")}</button>
        <button className={button} disabled={busy || ["pending", "recorded", "applying"].includes(e.status)} onClick={() => void forget(e.id)}>{t("undo.forget")}</button>
      </li>)}</ol>
    </div>
  </AppDialog>;
}
