import type { MonitorSettings } from "@/ipc/bindings";
import { useT } from "@/store/settingsStore";

const defaults: MonitorSettings = { enabled: true, intervalSeconds: 30, timeoutSeconds: 10, autoReconnect: true, maxAttempts: 3 };
export function MonitorSettingsForm({ value, onChange }: {
  value?: MonitorSettings;
  onChange: (value: MonitorSettings) => void;
}) {
  const t = useT();
  const settings = value ?? defaults;
  const set = (patch: Partial<MonitorSettings>) => onChange({ ...settings, ...patch });
  return <details className="rounded border border-edge p-2 text-xs">
    <summary className="cursor-pointer">{t("net.monitor.settings")}</summary>
    <div className="mt-2 grid gap-2">
      <label><input type="checkbox" checked={settings.enabled} onChange={e => set({ enabled: e.target.checked })} /> {t("net.monitor.enabled")}</label>
      <label>{t("net.monitor.interval")} <input type="number" min={5} max={3600} value={settings.intervalSeconds} onChange={e => set({ intervalSeconds: Number(e.target.value) })} /></label>
      <label>{t("net.monitor.timeout")} <input type="number" min={1} max={120} value={settings.timeoutSeconds} onChange={e => set({ timeoutSeconds: Number(e.target.value) })} /></label>
      <label><input type="checkbox" checked={settings.autoReconnect} onChange={e => set({ autoReconnect: e.target.checked })} /> {t("net.monitor.auto")}</label>
      <label>{t("net.monitor.attempts")} <input type="number" min={1} max={10} value={settings.maxAttempts} onChange={e => set({ maxAttempts: Number(e.target.value) })} /></label>
      <p className="text-dim">{t("net.monitor.notice")}</p>
    </div>
  </details>;
}
