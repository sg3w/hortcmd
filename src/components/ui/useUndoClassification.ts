import { useEffect, useState } from "react";
import { classifyUndo, type UndoRequest } from "@/ipc/domains/undo";
import type { UndoClassification } from "@/ipc/bindings/UndoClassification";
export function useUndoClassification(request?: UndoRequest): UndoClassification | null | undefined {
  const key = request ? JSON.stringify(request) : "";
  const [state, setState] = useState<{ key: string; value: UndoClassification } | null>(null);
  useEffect(() => {
    if (!key) return;
    let live = true;
    void classifyUndo(JSON.parse(key) as UndoRequest).catch(() => "irreversible" as const).then(value => { if (live) setState({ key, value }); });
    return () => { live = false; };
  }, [key]);
  return !key ? undefined : state?.key === key ? state.value : null;
}
