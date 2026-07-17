// ============================================================
// Display of running transfers (copy/move).
//
// TransferWindow: non-blocking window with two bars (top: current
//   file, bottom: overall operation) + "send to background".
//   TransferTray: minimized operations in the status bar; a click
//   restores the window.
// ============================================================

import {
  AlertTriangle,
  Ban,
  CheckCircle2,
  Copy,
  FolderInput,
  Hourglass,
  Minimize2,
  Package,
  PackageOpen,
  Pause,
  Play,
  X,
  type LucideIcon,
} from "lucide-react";
import { useTransfers, type Transfer } from "@/store/transfersStore";
import { cancelTransfer, pauseTransfer } from "@/ipc/client";
import { dequeueTransfer } from "@/features/commander/fileOps";
import { useT } from "@/store/settingsStore";
import type { TransKey } from "@/i18n/dictionaries";
import { formatSize } from "@/lib/format";

const OP_ICON: Record<Transfer["op"], LucideIcon> = {
  copy: Copy,
  move: FolderInput,
  extract: PackageOpen,
  pack: Package,
};

const OP_LABEL: Record<Transfer["op"], TransKey> = {
  copy: "fn.copy",
  move: "fn.move",
  extract: "op.extract",
  pack: "op.pack",
};

function pct(done: number, total: number): number {
  return total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
}

function opLabel(op: Transfer["op"]): TransKey {
  return OP_LABEL[op];
}

// ---------- Window (foreground) ----------

export function TransferWindow() {
  const transfers = useTransfers((s) => s.transfers);
  const visible = transfers.filter((t) => !t.minimized);
  if (visible.length === 0) return null;

  return (
    <div className="fixed bottom-16 right-4 z-50 flex flex-col gap-2">
      {visible.map((t) => (
        <TransferCard key={t.id} t={t} />
      ))}
    </div>
  );
}

function TransferCard({ t }: { t: Transfer }) {
  const minimize = useTransfers((s) => s.minimize);
  const remove = useTransfers((s) => s.remove);
  const setPaused = useTransfers((s) => s.setPaused);
  const tr = useT();
  const Icon = OP_ICON[t.op];
  const hasErrors = t.done && t.errors.length > 0;

  // Toggle pause/resume (optimistically + inform the backend).
  const togglePause = () => {
    const next = !t.paused;
    setPaused(t.id, next);
    pauseTransfer(t.id, next);
  };
  // Cancel: only remove waiting operations from the queue (no backend op).
  const onCancel = () => (t.queued ? dequeueTransfer(t.id) : cancelTransfer(t.id));

  const statusText = t.cancelled
    ? tr("op.cancelled")
    : t.queued
      ? tr("op.queued")
      : t.paused
        ? tr("op.paused")
        : tr(opLabel(t.op));

  return (
    <div className="w-[380px] max-w-[92vw] rounded-lg border border-edge bg-panel shadow-2xl">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-edge px-3 py-2 text-text">
        {t.cancelled ? (
          <Ban size={15} className="text-dim" />
        ) : t.done && !hasErrors ? (
          <CheckCircle2 size={15} className="text-emerald-400" />
        ) : hasErrors ? (
          <AlertTriangle size={15} className="text-red-400" />
        ) : t.queued ? (
          <Hourglass size={15} className="text-dim" />
        ) : t.paused ? (
          <Pause size={15} className="text-amber-400" />
        ) : (
          <Icon size={15} className="text-accent" />
        )}
        <span className="text-[13px]">
          {statusText}
          {!t.cancelled && t.filesTotal > 0 && (
            <span className="ml-2 text-dim">
              {t.filesDone}/{t.filesTotal} {tr("op.files")}
            </span>
          )}
        </span>
        <div className="ml-auto flex items-center gap-1">
          {!t.done && (
            <>
              {!t.queued && (
                <button
                  title={t.paused ? tr("op.resume") : tr("op.pause")}
                  onClick={togglePause}
                  className="rounded p-1 text-dim hover:bg-accent-dim hover:text-text"
                >
                  {t.paused ? <Play size={14} /> : <Pause size={14} />}
                </button>
              )}
              <button
                title={tr("op.cancel")}
                onClick={onCancel}
                className="rounded p-1 text-dim hover:bg-red-500/20 hover:text-red-400"
              >
                <Ban size={14} />
              </button>
              {!t.queued && (
                <button
                  title={tr("op.background")}
                  onClick={() => minimize(t.id, true)}
                  className="rounded p-1 text-dim hover:bg-accent-dim hover:text-text"
                >
                  <Minimize2 size={14} />
                </button>
              )}
            </>
          )}
          {t.done && (
            <button
              title={tr("op.close")}
              onClick={() => remove(t.id)}
              className="rounded p-1 text-dim hover:bg-accent-dim hover:text-text"
            >
              <X size={14} />
            </button>
          )}
        </div>
      </div>

      {/* Bars */}
      <div className="flex flex-col gap-3 px-3 py-3">
        <Bar
          label={tr("op.currentFile")}
          sub={basename(t.fileName)}
          value={pct(t.fileDone, t.fileTotal)}
        />
        <Bar
          label={tr("op.overall")}
          sub={`${formatSize(t.bytesDone)} / ${formatSize(t.bytesTotal)}`}
          value={pct(t.bytesDone, t.bytesTotal)}
          accent
        />
        {hasErrors && (
          <div className="max-h-24 overflow-y-auto whitespace-pre-line rounded bg-panel-inactive p-2 text-[11px] text-red-300">
            {t.errors.slice(0, 6).join("\n")}
          </div>
        )}
      </div>
    </div>
  );
}

function Bar({
  label,
  sub,
  value,
  accent,
}: {
  label: string;
  sub: string;
  value: number;
  accent?: boolean;
}) {
  return (
    <div>
      <div className="mb-1 flex justify-between text-[11px]">
        <span className="text-dim">{label}</span>
        <span className="font-mono text-dim">{value}%</span>
      </div>
      <div className="h-2 overflow-hidden rounded bg-panel-inactive">
        <div
          className={accent ? "h-full bg-accent" : "h-full bg-emerald-400"}
          style={{ width: `${value}%`, transition: "width 80ms linear" }}
        />
      </div>
      {sub && (
        <div className="mt-1 truncate font-mono text-[10px] text-dim">{sub}</div>
      )}
    </div>
  );
}

// ---------- Tray (minimized, in the status bar) ----------

export function TransferTray() {
  const transfers = useTransfers((s) => s.transfers);
  const minimize = useTransfers((s) => s.minimize);
  const minimized = transfers.filter((t) => t.minimized);
  if (minimized.length === 0) return null;

  return (
    <div className="flex items-center gap-2">
      {minimized.map((t) => {
        const Icon = OP_ICON[t.op];
        return (
          <button
            key={t.id}
            onClick={() => minimize(t.id, false)}
            title={basename(t.fileName)}
            className="flex items-center gap-1.5 rounded border border-edge bg-panel px-2 py-0.5 hover:border-accent"
          >
            <Icon size={12} className="shrink-0 text-accent" />
            {/* two narrow bars stacked on top of each other */}
            <span className="flex w-16 flex-col gap-0.5">
              <MiniBar value={pct(t.fileDone, t.fileTotal)} className="bg-emerald-400" />
              <MiniBar value={pct(t.bytesDone, t.bytesTotal)} className="bg-accent" />
            </span>
            <span className="font-mono text-[10px] text-dim">
              {pct(t.bytesDone, t.bytesTotal)}%
            </span>
          </button>
        );
      })}
    </div>
  );
}

function MiniBar({ value, className }: { value: number; className: string }) {
  return (
    <span className="block h-1 overflow-hidden rounded bg-panel-inactive">
      <span
        className={`block h-full ${className}`}
        style={{ width: `${value}%` }}
      />
    </span>
  );
}

function basename(path: string): string {
  if (!path) return "";
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}
