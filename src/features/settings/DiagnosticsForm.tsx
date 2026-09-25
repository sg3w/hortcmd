import { useEffect, useState } from "react";
import { diagnosticsClear, diagnosticsExport, diagnosticsLevel, diagnosticsStatus, hasTauri } from "@/ipc/client";
import type { DiagnosticStatus } from "@/ipc/bindings/DiagnosticStatus";
import type { DiagnosticLevel } from "@/ipc/bindings/DiagnosticLevel";
import { useT } from "@/store/settingsStore";
import { smallBtn } from "@/components/ui/settings/controls";
export function DiagnosticsForm() {
  const t = useT();
  const [status, setStatus] = useState<DiagnosticStatus | null>(null);
  const [destination, setDestination] = useState("");
  const [includeLocation, setIncludeLocation] = useState(false);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const refresh = () => diagnosticsStatus().then(setStatus);
  useEffect(() => { if (hasTauri) { let disposed = false; void diagnosticsStatus().then(s => { if (!disposed) setStatus(s); }).catch(() => { if (!disposed) setMessage(t("diagnostics.error")); }); return () => { disposed = true; }; } }, []);
  const run = async (action: () => Promise<unknown>) => { setBusy(true); setMessage(""); try { await action(); await refresh(); setMessage(t("diagnostics.done")); } catch { setMessage(t("diagnostics.error")); } finally { setBusy(false); } };
  if (!hasTauri) return <p>{t("diagnostics.desktop")}</p>;
  return <div className="space-y-3 text-sm">
    <p>{t("diagnostics.privacy")}</p>
    {message && <p role="status">{message}</p>}
    {status && <>
      <p>{status.version} · {status.platform} · {status.architecture} · {status.build}</p>
      <p className="break-all">{t("diagnostics.location")}: {status.location}</p>
      {!status.available && <p role="alert">{t("diagnostics.unavailable")}</p>}
      <label className="block">{t("diagnostics.level")} <select disabled={busy} value={status.level} onChange={e => void run(() => diagnosticsLevel(e.target.value as DiagnosticLevel))}>{(["error", "warn", "info", "debug"] as const).map(level => <option key={level} value={level}>{level}</option>)}</select></label>
      <h3>{t("diagnostics.categories")}</h3>
      <ul>{status.categories.map(c => <li key={c}>{c}</li>)}</ul>
      <ul>{status.configuration.map(c => <li key={c}>{c}</li>)}</ul>
      <ul>{status.providers.map(c => <li key={c}>{c}</li>)}</ul>
      <h3>{t("diagnostics.errors")}</h3>
      <ul className="max-h-32 overflow-auto font-mono text-xs">{status.recent.map((r,i) => <li key={i}>{r.code} · {r.correlation}</li>)}</ul>
    </>}
    <label className="block">{t("diagnostics.destination")}<input className="block w-full border border-edge bg-panel" value={destination} onChange={e => setDestination(e.target.value)} /></label>
    <label className="flex gap-2"><input type="checkbox" checked={includeLocation} onChange={e => setIncludeLocation(e.target.checked)} />{t("diagnostics.identifiers")}</label>
    <div className="flex flex-wrap gap-2">
      <button disabled={busy || !destination || !status} className={smallBtn} onClick={() => void run(() => diagnosticsExport(destination, includeLocation))}>{t("diagnostics.export")}</button>
      <button disabled={busy} className={smallBtn} onClick={() => void run(diagnosticsClear)}>{t("diagnostics.clear")}</button>
      <button disabled={busy} className={smallBtn} onClick={() => void run(refresh)}>{t("diagnostics.refresh")}</button>
    </div>
  </div>;
}
