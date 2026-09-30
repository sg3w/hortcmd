// ============================================================
// Settings ▸ File operations (TICKET-035).
//
// The safety and speed of copying: failure-safe transfers (ticket 029),
// checksum verification, the trash, the speed limit, the buffer size
// and the worker count.
// ============================================================

import { useSettings, useT } from "@/store/settingsStore";
import { Beta, Toggle } from "./controls";
import {
  Trash2,
  FileCheck2,
  HardDriveDownload,
  ShieldAlert,
  ShieldCheck,
  Gauge,
  ListOrdered,
  Cpu,
  Undo2,
  History,
} from "lucide-react";

export function OperationsForm() {
  const t = useT();
  const useTrash = useSettings((s) => s.useTrash);
  const setUseTrash = useSettings((s) => s.setUseTrash);
  const queueTransfers = useSettings((s) => s.queueTransfers);
  const setQueueTransfers = useSettings((s) => s.setQueueTransfers);
  const verifyCopies = useSettings((s) => s.verifyCopies);
  const setVerifyCopies = useSettings((s) => s.setVerifyCopies);
  const speedLimit = useSettings((s) => s.speedLimit);
  const setSpeedLimit = useSettings((s) => s.setSpeedLimit);
  const bufferSizeKb = useSettings((s) => s.bufferSizeKb);
  const setBufferSizeKb = useSettings((s) => s.setBufferSizeKb);
  const copyThreads = useSettings((s) => s.copyThreads);
  const setCopyThreads = useSettings((s) => s.setCopyThreads);
  const safeTransfers = useSettings((s) => s.safeTransfers);
  const setSafeTransfers = useSettings((s) => s.setSafeTransfers);

  const checkSpace = useSettings(s => s.checkTransferSpace);
  const setCheckSpace = useSettings(s => s.setCheckTransferSpace);
  const undoEnabled = useSettings((s) => s.undoEnabled);
  const setUndoEnabled = useSettings((s) => s.setUndoEnabled);
  const recoveryEnabled = useSettings((s) => s.recoveryEnabled);
  const setRecoveryEnabled = useSettings((s) => s.setRecoveryEnabled);

  return (
    <div className="flex flex-col gap-4">
      <Toggle
        checked={useTrash}
        onChange={setUseTrash}
        icon={Trash2}
        label={t("settings.trash")}
        hint={t("settings.trash.hint")}
      />
      <Toggle
        checked={queueTransfers}
        onChange={setQueueTransfers}
        icon={ListOrdered}
        label={t("settings.queue")}
        hint={t("settings.queue.hint")}
      />
      <Toggle
        checked={verifyCopies}
        onChange={setVerifyCopies}
        icon={ShieldCheck}
        label={t("settings.verify")}
        hint={t("settings.verify.hint")}
      />
      <Toggle
        checked={checkSpace}
        onChange={setCheckSpace}
        icon={HardDriveDownload}
        label={t("settings.checkTransferSpace")}
        hint={t("settings.checkTransferSpace.hint")}
      />
      {/* Failure-safe transfers (TICKET-029). Opt-in, so the risk of the
          default state is spelled out rather than left implicit. */}
      <div className="flex flex-col gap-2">
        <Toggle
          checked={safeTransfers}
          onChange={setSafeTransfers}
          icon={FileCheck2}
          label={t("settings.safeTransfers")}
          hint={t("settings.safeTransfers.hint")}
        />
        {!safeTransfers && (
          <div className="ml-[27px] flex items-start gap-2 rounded border border-amber-500/40 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-400">
            <ShieldAlert size={14} className="mt-px shrink-0" />
            <span>{t("settings.safeTransfers.warning")}</span>
          </div>
        )}
      </div>
      {/* File undo (TICKET-391). Beta and off by default: while it is on,
          every file operation writes a journal entry and undone copies
          leave a retained backup behind, so the state that costs the user
          something is the one that explains itself. */}
      <div className="flex flex-col gap-2">
        <Toggle
          checked={undoEnabled}
          onChange={setUndoEnabled}
          icon={Undo2}
          label={t("settings.undo")}
          hint={t("settings.undo.hint")}
          badge={<Beta />}
        />
        {undoEnabled && (
          <div className="ml-[27px] flex items-start gap-2 rounded border border-amber-500/40 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-400">
            <ShieldAlert size={14} className="mt-px shrink-0" />
            <span>{t("settings.undo.warning")}</span>
          </div>
        )}
      </div>

      {/* Transfer recovery (TICKET-396). Beta and off by default: while it is
          on, every transfer writes a journal record and an interruption stays
          on the list until it is answered, so — as with file undo — the state
          that costs the user something is the one that explains itself. */}
      <div className="flex flex-col gap-2">
        <Toggle
          checked={recoveryEnabled}
          onChange={setRecoveryEnabled}
          icon={History}
          // The dialog's own title: one name for the feature, in one place.
          label={t("recovery.title")}
          hint={t("settings.recovery.hint")}
          badge={<Beta />}
        />
        {recoveryEnabled && (
          <div className="ml-[27px] flex items-start gap-2 rounded border border-amber-500/40 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-400">
            <ShieldAlert size={14} className="mt-px shrink-0" />
            <span>{t("settings.recovery.warning")}</span>
          </div>
        )}
      </div>

      {/* Speed limit (0 = unlimited). */}
      <label className="flex items-start gap-3">
        <Gauge size={15} className="mt-0.5 shrink-0 text-dim" />
        <span className="flex flex-col gap-1">
          <span className="text-[13px] text-text">{t("settings.speed")}</span>
          <span className="text-[11px] text-dim">{t("settings.speed.hint")}</span>
          <div className="mt-1 flex items-center gap-2">
            <input
              type="number"
              min={0}
              value={speedLimit}
              onChange={(e) =>
                setSpeedLimit(Math.max(0, Math.floor(Number(e.target.value) || 0)))
              }
              className="w-28 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
            />
            <span className="text-[12px] text-dim">KB/s</span>
          </div>
        </span>
      </label>

      {/* Buffer size (KB). */}
      <label className="flex items-start gap-3">
        <HardDriveDownload size={15} className="mt-0.5 shrink-0 text-dim" />
        <span className="flex flex-col gap-1">
          <span className="text-[13px] text-text">{t("settings.buffer")}</span>
          <span className="text-[11px] text-dim">{t("settings.buffer.hint")}</span>
          <div className="mt-1 flex items-center gap-2">
            <input
              type="number"
              min={0}
              value={bufferSizeKb}
              onChange={(e) => setBufferSizeKb(Number(e.target.value) || 0)}
              className="w-28 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
            />
            <span className="text-[12px] text-dim">KB</span>
          </div>
        </span>
      </label>

      {/* Parallel copy threads (1 = sequential). */}
      <label className="flex items-start gap-3">
        <Cpu size={15} className="mt-0.5 shrink-0 text-dim" />
        <span className="flex flex-col gap-1">
          <span className="text-[13px] text-text">{t("settings.threads")}</span>
          <span className="text-[11px] text-dim">
            {t("settings.threads.hint")}
          </span>
          <div className="mt-1 flex items-center gap-2">
            <input
              type="number"
              min={1}
              max={16}
              value={copyThreads}
              onChange={(e) => setCopyThreads(Number(e.target.value) || 1)}
              className="w-28 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
            />
          </div>
        </span>
      </label>
    </div>
  );
}
