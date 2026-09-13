import { useEffect, useState } from "react";
import { useTransfers, type Transfer } from "@/store/transfersStore";
import { useT } from "@/store/settingsStore";
import { fillVars } from "@/store/noticeStore";
import { OP_LABEL, percent, transferState } from "./transferPresentation";

interface Announcement { transfer: Transfer; progress: boolean }

/** State changes are immediate; progress is sampled at most every ten seconds.
 * Mounted independently of foreground cards, so background transfers speak too.
 * Keep snapshots as data so changing language also translates the current text.
 */
export function TransferAnnouncements() {
  const [announcements, setAnnouncements] = useState<Announcement[]>([]);
  const t = useT();
  useEffect(() => {
    let previous = new Map<string, Transfer>();
    const progressSeen = new Map<string, number>();
    const onChange = () => {
      const current = useTransfers.getState().transfers;
      const next = new Map(current.map(item => [item.id, item]));
      const changes: Announcement[] = [];
      for (const item of current) {
        const old = previous.get(item.id);
        if (!old || transferState(old) !== transferState(item)) {
          changes.push({ transfer: item, progress: false });
          progressSeen.set(item.id, percent(item.bytesDone, item.bytesTotal));
        }
      }
      for (const [id, old] of previous) {
        if (!next.has(id)) {
          // Cancelling a queued operation removes it without a backend event.
          if (old.queued && !old.done) changes.push({ transfer: { ...old, cancelled: true }, progress: false });
          progressSeen.delete(id);
        }
      }
      previous = next;
      if (changes.length) setAnnouncements(changes);
    };
    onChange();
    const unsubscribe = useTransfers.subscribe(onChange);
    const timer = window.setInterval(() => {
      const changes: Announcement[] = [];
      for (const item of useTransfers.getState().transfers) {
        if (item.done || item.cancelled || item.paused || item.queued || item.bytesTotal <= 0) continue;
        const value = percent(item.bytesDone, item.bytesTotal);
        if (progressSeen.get(item.id) !== value) {
          changes.push({ transfer: item, progress: true });
          progressSeen.set(item.id, value);
        }
      }
      if (changes.length) setAnnouncements(changes);
    }, 10_000);
    return () => { unsubscribe(); window.clearInterval(timer); };
  }, []);

  return <div role="status" aria-live="polite" aria-atomic="true" className="sr-only">
    {announcements.map(({ transfer, progress }) => fillVars(t(progress ? "op.announceProgress" : "op.announceState"), {
      operation: t(OP_LABEL[transfer.op]),
      state: t(transferState(transfer)),
      name: transfer.fileName,
      percent: percent(transfer.bytesDone, transfer.bytesTotal),
    })).join("; ")}
  </div>;
}
