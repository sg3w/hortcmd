import type { Transfer } from "@/store/transfersStore";
import type { TransKey } from "@/i18n/dictionaries";

export const OP_LABEL: Record<Transfer["op"], TransKey> = {
  copy: "fn.copy", move: "fn.move", extract: "op.extract", pack: "op.pack",
};

export function transferState(t: Transfer): TransKey {
  if (t.cancelled) return "op.cancelled";
  if (t.done) return t.errors.length ? "op.errors" : "op.done";
  if (t.queued) return "op.queued";
  if (t.paused) return "op.paused";
  return "op.progress";
}

export function percent(done: number, total: number): number {
  return total > 0 ? Math.max(0, Math.min(100, Math.round((done / total) * 100))) : 0;
}
