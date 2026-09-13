import type { TransportInfo } from "@/ipc/bindings/TransportInfo";
import { useT } from "@/store/settingsStore";
export function TransportDetails({ info }: { info: TransportInfo }) {
  const t = useT();
  return <details className="mt-1 text-[11px]">
    <summary className={info.encryption ? "text-dim" : "text-amber-400"}>{t(`net.transport.${info.state}`)}</summary>
    <dl className="space-y-1 break-all py-1">
      <div><dt>{t("net.transport.protocol")}</dt><dd>{info.protocol ?? t("net.transport.unavailable")}</dd></div>
      <div><dt>{t("net.transport.cipher")}</dt><dd>{info.cipher ?? t("net.transport.unavailable")}</dd></div>
      <div><dt>{t("net.transport.verification")}</dt><dd>{t(`net.transport.${info.certificateStatus}`)}</dd></div>
      {info.signing !== null && <div><dt>{t("net.transport.signing")}</dt><dd>{t(info.signing ? "net.transport.required" : "net.transport.notRequired")}</dd></div>}
      {info.fingerprint && <div><dt>{t("net.certificate.fingerprint")}</dt><dd>{info.fingerprint}</dd></div>}
    </dl>
  </details>;
}
