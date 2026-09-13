import { useEffect, useRef, useState } from "react";
import type { Profile } from "@/ipc/bindings/Profile";
import type { ProfileSecurity } from "@/ipc/bindings/ProfileSecurity";
import type { CertificatePreview } from "@/ipc/bindings/CertificatePreview";
import { netCertificatePreview } from "@/ipc/client";
import { describeErrorFull } from "@/store/errorText";
import { useT } from "@/store/settingsStore";

export function CertificateTrustForm({ profile, onChange }: { profile: Profile; onChange: (value: ProfileSecurity) => void }) {
  const t = useT();
  const generation = useRef(0);
  const [path, setPath] = useState(profile.security.certificate?.path ?? "");
  const [preview, setPreview] = useState<CertificatePreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { generation.current += 1; setPreview(null); setBusy(false); return () => { generation.current += 1; }; }, [profile.id, profile.host, profile.port, profile.protocol, profile.user]);
  const inspect = async () => {
    const request = ++generation.current;
    setBusy(true); setError(null); setPreview(null);
    try { const candidate = await netCertificatePreview(profile, path); if (generation.current === request) setPreview(candidate); }
    catch (error) { if (generation.current === request) setError(describeErrorFull(error)); }
    finally { if (generation.current === request) setBusy(false); }
  };
  return <details className="rounded border border-edge p-2 text-xs">
    <summary>{t("net.certificate.title")}</summary>
    <p className="my-2">{t("net.certificate.help")}</p>
    {profile.security.certificate && <div className="my-2 break-all">
      <p>{profile.security.certificate.fingerprint}</p>
      <button type="button" onClick={() => { onChange({ ...profile.security, certificate: null }); setPreview(null); }}>{t("net.certificate.remove")}</button>
    </div>}
    <label className="block">{t("net.certificate.path")}<input value={path} onChange={e => { generation.current += 1; setBusy(false); setPath(e.target.value); setPreview(null); }} className="mt-1 w-full rounded border border-edge bg-panel px-2 py-1" /></label>
    <button type="button" disabled={busy || !path.trim()} onClick={() => void inspect()} className="my-2 rounded border border-edge px-2 py-1 disabled:opacity-50">{t("net.certificate.inspect")}</button>
    {error && <p role="alert">{error}</p>}
    {preview && <section className="space-y-2 break-all" aria-label={t("net.certificate.preview")}>
      <dl>{[
        [t("net.certificate.subject"), preview.subject], [t("net.certificate.issuer"), preview.issuer],
        [t("net.certificate.validFrom"), preview.validFrom], [t("net.certificate.validUntil"), preview.validUntil],
        [t("net.certificate.hostnames"), preview.hostnames.join(", ") || "—"],
        [t("net.certificate.algorithm"), preview.algorithm], [t("net.certificate.fingerprint"), preview.trust.fingerprint],
      ].map(([label, value]) => <div key={label}><dt className="text-dim">{label}</dt><dd>{value}</dd></div>)}</dl>
      <p>{t(preview.trust.isCa ? "net.certificate.caWarning" : "net.certificate.pinWarning")}</p>
      <button type="button" className="rounded border border-edge px-2 py-1" onClick={() => { onChange({ ...profile.security, certificate: preview.trust }); setPreview(null); }}>{t("net.certificate.approve")}</button>
    </section>}
  </details>;
}
