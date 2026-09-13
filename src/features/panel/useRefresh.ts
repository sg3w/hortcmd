import { useEffect } from "react";
import { hasTauri, refreshRead, refreshFinish, refreshPoll, refreshSubscribe } from "@/ipc/client";
import { beginRefresh, applyRefreshResult, applyRefreshStatus, usePanes } from "@/store/panesStore";
import { useNet } from "@/features/net/netStore";

/** UI ownership and visibility only; policy, paths and scheduling live in Rust. */
export function useRefresh() {
  useEffect(() => {
    if (!hasTauri) return;
    let disposed = false;
    let running = false;
    let again = false;
    let key = "";
    let timer: ReturnType<typeof setTimeout> | undefined;
    const targets = () => {
      const state = usePanes.getState();
      return [...state.left.tabs, ...state.right.tabs]
        .filter(tab => tab.path && !tab.archive && !tab.loading)
        .map(tab => ({id:tab.id, generation:tab.request, path:tab.path}));
    };
    const run = async () => {
      if (disposed) return;
      if (running) { again = true; return; }
      running = true;
      let delay = 250;
      let idle = false;
      try {
        const current = targets();
        idle = current.length === 0;
        const nextKey = JSON.stringify(current);
        if (nextKey !== key) { await refreshSubscribe(current); key = nextKey; }
        if (disposed || idle) return;
        const batch = await refreshPoll(document.visibilityState !== "hidden");
        if (disposed) return;
        batch.status.forEach(applyRefreshStatus);
        for (const work of batch.work) {
          work.targets.forEach(beginRefresh);
          void (async () => {
            let failure: string | null = null;
            try {
              const listing = await refreshRead(work.key, work.serial);
              if (!disposed) work.targets.forEach(target => applyRefreshResult(target, listing));
            } catch (error) {
              failure = String(error);
              if (!disposed) work.targets.forEach(target => applyRefreshResult(target, null, error));
            } finally {
              await refreshFinish(work.key, work.serial, failure).catch(() => {});
            }
          })();
        }
      } catch (error) {
        key = "";
        delay = 5000;
        if (!disposed) {
          await refreshSubscribe([]).catch(() => {});
          targets().forEach(target => applyRefreshResult(target, null, error));
        }
      } finally {
        running = false;
        if (!disposed && (again || (!idle && document.visibilityState !== "hidden"))) {
          timer = setTimeout(() => void run(), again ? 0 : delay);
        }
        again = false;
      }
    };
    const kick = () => { clearTimeout(timer); void run(); };
    // Do not resubscribe for selection/cursor/data changes.
    let previous = JSON.stringify(targets());
    const offPanes = usePanes.subscribe(() => {
      const current = JSON.stringify(targets());
      if (current !== previous) { previous = current; kick(); }
    });
    const offNet = useNet.subscribe(kick);
    document.addEventListener("visibilitychange", kick);
    kick();
    return () => {
      disposed = true;
      clearTimeout(timer);
      offPanes(); offNet();
      document.removeEventListener("visibilitychange", kick);
      void refreshSubscribe([]).catch(() => {});
    };
  }, []);
}
