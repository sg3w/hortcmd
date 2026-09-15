import { useEffect, useState } from "react";
import { cancelPreflight, transferPreflight, type PreflightRequest } from "@/ipc/domains/preflight";
import type { OperationPlan } from "@/ipc/bindings/OperationPlan";
import type { TransKey } from "@/i18n/dictionaries";
import { describeErrorFull } from "@/store/errorText";
import { useSettings, useT } from "@/store/settingsStore";
import { openSettings } from "@/store/uiStore";
import { formatSize } from "@/lib/format";
interface Result { key: string; plan: OperationPlan | null; error: string | null }
export function useTransferPreflight(request?: PreflightRequest) {
  const checkSpace = useSettings(s => s.checkTransferSpace);
  const key = request ? JSON.stringify({ ...request, settings: { ...request.settings, checkSpace } }) : "";
  const [result, setResult] = useState<Result | null>(null);
  useEffect(() => {
    if (!key) return;
    const token = crypto.randomUUID(); let live = true; let finished = false;
    void transferPreflight(token, JSON.parse(key) as PreflightRequest).then(plan => { if (live) setResult({ key, plan, error: null }); }).catch(error => { if (live) setResult({ key, plan: null, error: describeErrorFull(error) }); }).finally(() => { finished = true; });
    return () => { live = false; if (!finished) void cancelPreflight(token).catch(() => {}); };
  }, [key]);
  const current = result?.key === key ? result : null;
  const loading = !!key && !current;
  return { loading, plan: current?.plan ?? null, error: current?.error ?? null,
    blocked: loading || !!current?.error || !!current?.plan?.issues.some(i => i.severity === "blocking") };
}
const issues: Record<string, TransKey> = {
  noStaging: "preflight.noStaging", readOnly: "preflight.readOnly", capacityUnknown: "preflight.capacityUnknown", metadataUnknown: "preflight.metadataUnknown", namesUnknown: "preflight.namesUnknown", writableUnknown: "preflight.writableUnknown", insecure: "preflight.insecure", crossProvider: "preflight.crossProvider", selfCopy: "preflight.selfCopy", nestedSources: "preflight.nestedSources", duplicateTarget: "preflight.duplicateTarget", mixedSources: "preflight.mixedSources", invalidName: "preflight.invalidName", recursiveSource: "preflight.selfCopy", caseCollision: "preflight.duplicateTarget", objectLimit: "preflight.objectLimit", overflow: "preflight.planningLimit", sourceChanged: "preflight.sourceChanged", planningLimit: "preflight.planningLimit", unreadable: "preflight.unreadable", collision: "preflight.collision", noSpace: "preflight.noSpace", large: "preflight.large",
};
const size = (bytes: number) => formatSize(bytes).replace(/ ([KMGT])$/, " $1iB");
export function TransferPreflight({ loading, plan, error }: ReturnType<typeof useTransferPreflight>) {
  const t = useT();
  const unique = plan?.issues.filter((issue, index, all) => all.findIndex(other => other.code === issue.code && other.item === issue.item && other.severity === issue.severity) === index) ?? [];
  const urgent = unique.filter(i => i.severity !== "advisory" || i.code === "capacityUnknown");
  const extra = unique.filter(i => !urgent.includes(i));
  const list = (items: typeof unique) => <ul className="mt-1 space-y-1">{items.map((issue, i) => <li key={i} className={issue.severity === "blocking" ? "text-red-400" : undefined}>
    {issue.severity === "blocking" ? `${t("preflight.blocking")}: ` : ""}{t(issues[issue.code] ?? "preflight.unreadable")}{issue.item === null ? "" : ` (${issue.item + 1})`}
  </li>)}</ul>;
  return <section className="px-4 pb-3 text-sm" aria-label={t("preflight.title")} aria-busy={loading}>
    {loading && <p>{t("preflight.loading")}</p>}
    {error && <p role="alert">{error}</p>}
    {plan && <>
      <p>{t("preflight.summary").replace("{files}", String(plan.mappings.reduce((sum, m) => sum + m.files, 0))).replace("{bytes}", size(plan.space.additionalBytes))}</p>
      {list(urgent)}
      <p className="mt-2 text-xs text-dim">
        {plan.settings.checkSpace === false && <span>{t("preflight.disabled")} · </span>}
        <button type="button" className="text-accent underline" onClick={() => openSettings("operations")}>{t("preflight.settings")}</button>
      </p>
      <details className="mt-2 text-xs text-dim">
        <summary className="cursor-pointer">{t("preflight.details")}</summary>
        <p>{t("preflight.peak").replace("{bytes}", size(plan.space.peakFootprintBytes))}</p>
        {plan.settings.checkSpace !== false && <p>{t("preflight.capacity").replace("{bytes}", plan.limits.availableBytes === null ? t("preflight.unknown") : size(plan.limits.availableBytes)).replace("{quota}", plan.limits.quotaRemainingBytes === null ? t("preflight.unknown") : size(plan.limits.quotaRemainingBytes))}</p>}
        <p>{t("preflight.mode").replace("{mode}", plan.settings.safe ? t("preflight.safe") : t("preflight.direct")).replace("{staging}", size(plan.space.stagingBytes)).replace("{backup}", size(plan.space.backupBytes)).replace("{verify}", size(plan.space.verificationReadBytes))}</p>
        {list(extra)}
      </details>
    </>}
  </section>;
}
