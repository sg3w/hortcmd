import { useEffect, useState } from "react";
import type { ConnectionStatus } from "@/ipc/bindings";
import { useT } from "@/store/settingsStore";
import { useNet } from "./netStore";
import { phaseLabel } from "./lifecycleUi";
import { describeFailure } from "./connFailure";

function signature(c: ConnectionStatus): string {
  return `${c.monitor ?? ""}:${c.phase}:${c.phase === "failed" ? c.error?.error.id ?? "" : ""}`;
}

/** Speak lifecycle changes once, not every backend status refresh. */
export function ConnectionAnnouncements() {
  const [changes, setChanges] = useState<ConnectionStatus[]>([]);
  const t = useT();
  useEffect(() => {
    let previous = new Map(useNet.getState().connections.map(c => [c.id, signature(c)]));
    return useNet.subscribe(state => {
      const changed = state.connections.filter(c => previous.get(c.id) !== signature(c)
        && (previous.has(c.id) || c.phase !== "disconnected"));
      previous = new Map(state.connections.map(c => [c.id, signature(c)]));
      if (changed.length) setChanges(changed);
    });
  }, []);
  return <div role="status" aria-live="polite" aria-atomic="true" className="sr-only">
    {changes.map(c => `${c.name}: ${t(c.monitor === "interrupted" ? "net.monitor.interrupted" : c.monitor === "reconnecting" ? "net.monitor.reconnecting" : phaseLabel(c.phase))}${c.phase === "failed" && c.error ? `. ${describeFailure(c.error)}` : ""}`).join("; ")}
  </div>;
}
