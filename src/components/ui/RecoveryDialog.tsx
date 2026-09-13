import { useState } from "react";
import type { RecoveryEntry } from "@/ipc/bindings/RecoveryEntry";
import type { RecoveryLocation } from "@/ipc/bindings/RecoveryLocation";
import { acknowledgeRecovery, cleanupRecovery, discardRecovery, prepareRecovery, recoveryDestination } from "@/ipc/client";
import { startRecovered } from "@/features/commander/fileOps";
import { useRecovery } from "@/store/recoveryStore";
import { usePanes } from "@/store/panesStore";
import { describeErrorFull } from "@/store/errorText";
import { useT } from "@/store/settingsStore";
import { AppDialog } from "./AppDialog";

type Action = "cleanup" | "discard" | "retry";
export function RecoveryDialog() {
  const { open, setOpen, entries, refresh, loading, error } = useRecovery();
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<{ entry: RecoveryEntry; action: Action } | null>(null);
  const location = (value: RecoveryLocation) => value.kind === "local" ? value.path : `${t("recovery.profiles")} ${value.profile}: ${value.path}`;
  const perform = async (action: () => Promise<void>) => {
    setBusy(true); setFailure(null);
    try { await action(); await refresh(); setConfirmation(null); }
    catch (error) { setFailure(describeErrorFull(error)); }
    finally { setBusy(false); }
  };
  const retry = async (entry: RecoveryEntry, resume: boolean) => startRecovered(await prepareRecovery(entry.id, resume));
  const confirm = async () => {
    if (!confirmation) return;
    const { action, entry } = confirmation;
    if (action === "cleanup") await cleanupRecovery(entry.id);
    else if (action === "discard") await discardRecovery(entry.id);
    else await retry(entry, false);
  };
  const buttonClass = "rounded border border-edge px-2 py-1 disabled:opacity-50";
  return <AppDialog dialogKey="transferRecovery" open={open} onClose={() => { if (!busy) { setConfirmation(null); setOpen(false); } }}
    titleBar={t("recovery.title")} defaultSize={{ w: 740, h: 500 }} minSize={{ w: 360, h: 260 }}
    footer={<><button className={buttonClass} disabled={busy || loading} onClick={() => void refresh()}>{t("recovery.refresh")}</button><button className={buttonClass} disabled={busy} onClick={() => setOpen(false)}>{t("recovery.close")}</button></>}>
    <div className="min-h-0 flex-1 overflow-auto p-3" aria-busy={busy || loading}>
      <p className="mb-3 text-sm text-dim">{t("recovery.help")}</p>
      {(failure || error) && <p role="alert" className="mb-3 whitespace-pre-wrap text-error">{failure || error}</p>}
      {confirmation ? <section className="space-y-3" aria-label={t("recovery.confirm")}>
        <p>{t(confirmation.action === "cleanup" ? "recovery.confirmCleanup" : confirmation.action === "discard" ? "recovery.confirmDiscard" : "recovery.confirmRetry")}</p>
        <p className="break-all">{location(confirmation.entry.destination)}</p>
        <div className="flex gap-2"><button autoFocus className={buttonClass} disabled={busy} onClick={() => setConfirmation(null)}>{t("recovery.cancel")}</button><button className={buttonClass} disabled={busy} onClick={() => void perform(confirm)}>{t("recovery.confirm")}</button></div>
      </section> : <>
        {!loading && entries.length === 0 && <p>{t("recovery.empty")}</p>}
        <ol className="space-y-3">{entries.map(entry => <li key={entry.id} className="rounded border border-edge p-3">
          <h3 className="font-semibold">{t(({ copy: "fn.copy", move: "fn.move", extract: "op.extract", pack: "op.pack" } as const)[entry.kind])} — {t(`recovery.${entry.state}`)}</h3>
          <p className="break-all" title={location(entry.destination)}>{location(entry.destination)}</p>
          <ul className="text-sm text-dim">{entry.sources.map((source, index) => <li key={index} className="break-all">{location(source)}</li>)}</ul>
          <p className="text-sm">{t("recovery.progress").replace("{done}", String(entry.committed)).replace("{total}", String(entry.total))}</p>
          <div className="mt-2 flex flex-wrap gap-2">
            {(["queued", "resumable"] as string[]).includes(entry.state) && <button className={buttonClass} disabled={busy} onClick={() => void perform(() => retry(entry, true))}>{t("recovery.resume")}</button>}
            {(["retryable", "unsafeInterrupted"] as string[]).includes(entry.state) && <button className={buttonClass} disabled={busy} onClick={() => setConfirmation({ entry, action: "retry" })}>{t("recovery.retry")}</button>}
            {entry.state === "cleanupRequired" && <button className={buttonClass} disabled={busy} onClick={() => setConfirmation({ entry, action: "cleanup" })}>{t("recovery.cleanup")}</button>}
            <button className={buttonClass} disabled={busy || entry.state === "unavailable"} onClick={() => void perform(async () => {
              const path = await recoveryDestination(entry.id); await usePanes.getState().loadDir(usePanes.getState().active, path);
            })}>{t("recovery.reveal")}</button>
            {entry.state === "completed" ? <button className={buttonClass} disabled={busy} onClick={() => void perform(() => acknowledgeRecovery(entry.id))}>{t("recovery.ack")}</button>
              : <button className={buttonClass} disabled={busy || entry.state === "running"} onClick={() => setConfirmation({ entry, action: "discard" })}>{t("recovery.discard")}</button>}
          </div>
        </li>)}</ol>
      </>}
    </div>
  </AppDialog>;
}
