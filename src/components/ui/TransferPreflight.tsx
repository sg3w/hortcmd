import { useEffect, useState } from "react";
import { cancelPreflight, transferPreflight, type PreflightRequest } from "@/ipc/domains/preflight";
import type { OperationPlan } from "@/ipc/bindings/OperationPlan";
import type { TransKey } from "@/i18n/dictionaries";
import { describeErrorFull } from "@/store/errorText";
import { useT } from "@/store/settingsStore";
interface Result { key: string; plan: OperationPlan | null; error: string | null }
export function useTransferPreflight(request?: PreflightRequest) {
  const key = request ? JSON.stringify(request) : "";
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
export function TransferPreflight({ loading, plan, error }: ReturnType<typeof useTransferPreflight>) {
  const t = useT();
  return <section className="px-4 pb-3 text-sm" aria-label={t("preflight.title")} aria-busy={loading}>
    {loading && <p>{t("preflight.loading")}</p>}
    {error && <p role="alert">{error}</p>}
    {plan && <>
      <p>{t("preflight.summary").replace("{files}", String(plan.mappings.reduce((sum,m) => sum + m.files, 0))).replace("{bytes}", String(plan.space.additionalBytes)).replace("{peak}", String(plan.space.peakFootprintBytes))}</p>
      <p>{t("preflight.capacity").replace("{bytes}", plan.limits.availableBytes === null ? t("preflight.unknown") : String(plan.limits.availableBytes)).replace("{quota}", plan.limits.quotaRemainingBytes === null ? t("preflight.unknown") : String(plan.limits.quotaRemainingBytes))}</p>
      <p>{t("preflight.mode").replace("{mode}", plan.settings.safe ? t("preflight.safe") : t("preflight.direct")).replace("{staging}", String(plan.space.stagingBytes)).replace("{backup}", String(plan.space.backupBytes)).replace("{verify}", String(plan.space.verificationReadBytes))}</p>
      <ul className="mt-2 space-y-1">{plan.issues.map((issue,i) => <li key={i}>{t(issue.severity === "blocking" ? "preflight.blocking" : issue.severity === "decision" ? "preflight.decision" : "preflight.advisory")}: {t(issues[issue.code] ?? "preflight.unreadable")}{issue.item === null ? "" : ` (${issue.item + 1})`}</li>)}</ul>
    </>}
  </section>;
}
