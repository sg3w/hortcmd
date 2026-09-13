import { useEffect, useState } from "react";
import type { ProfileSecurity } from "@/ipc/bindings/ProfileSecurity";
import { netSetTransportPolicy, netTransportPolicy } from "@/ipc/client";
import { describeErrorFull } from "@/store/errorText";
import { useT } from "@/store/settingsStore";

export function TransportSecurityForm({ value, onChange }: {
  value: ProfileSecurity;
  onChange: (value: ProfileSecurity) => void;
}) {
  const t = useT();
  const [allowed, setAllowed] = useState(false);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    void netTransportPolicy().then(policy => { if (live) setAllowed(policy.allowPlaintext); })
      .catch(error => { if (live) setError(describeErrorFull(error)); })
      .finally(() => { if (live) setBusy(false); });
    return () => { live = false; };
  }, []);
  const changeGlobal = async (allowPlaintext: boolean) => {
    setBusy(true); setError(null);
    try { await netSetTransportPolicy({ allowPlaintext }); setAllowed(allowPlaintext); }
    catch (error) { setError(describeErrorFull(error)); }
    finally { setBusy(false); }
  };
  return <details className="rounded border border-edge p-2 text-xs">
    <summary>{t("net.security.title")}</summary>
    <p className="my-2 text-amber-400">{t("net.security.warning")}</p>
    {error && <p role="alert">{error}</p>}
    <label className="mb-2 flex items-start gap-2">
      <input type="checkbox" checked={allowed} disabled={busy} onChange={e => void changeGlobal(e.target.checked)} />
      {t("net.security.global")}
    </label>
    <label className="flex items-start gap-2">
      <input type="checkbox" checked={value.plaintextAcknowledged} disabled={busy || !allowed}
        onChange={e => onChange({ ...value, plaintextAcknowledged: e.target.checked })} />
      {t("net.security.profile")}
    </label>
  </details>;
}
