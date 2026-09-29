import { useEffect, useState } from "react";
import { classifyUndo, type UndoRequest } from "@/ipc/domains/undo";
import { useSettings } from "@/store/settingsStore";
import type { UndoClassification } from "@/ipc/bindings/UndoClassification";
export function useUndoClassification(request?: UndoRequest): UndoClassification | null | undefined {
  // Beta and off by default (TICKET-391). While it is off nothing is
  // recorded, so the dialogs say nothing about reversibility rather than
  // explaining a feature that is not running — `undefined` is exactly
  // that: no sentence at all.
  const enabled = useSettings((s) => s.undoEnabled);
  const key = request && enabled ? JSON.stringify(request) : "";
  const [state, setState] = useState<{ key: string; value: UndoClassification } | null>(null);
  useEffect(() => {
    if (!key) return;
    let live = true;
    void classifyUndo(JSON.parse(key) as UndoRequest).catch(() => "irreversible" as const).then(value => { if (live) setState({ key, value }); });
    return () => { live = false; };
  }, [key]);
  return !key ? undefined : state?.key === key ? state.value : null;
}
