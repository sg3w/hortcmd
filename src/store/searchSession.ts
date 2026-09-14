import { useCallback } from "react";
import { create } from "zustand";
import type { SearchHit, SearchStatus } from "@/ipc/bindings";
import type { SearchMode } from "@/ipc/client";
import type { Side } from "./panesStore";

const emptyResults = { hits: [] as SearchHit[], running: false, status: null as SearchStatus | null,
  failure: null as string | null, blocked: null as string | null, skipped: 0, ran: false };
export const useSearchSession = create(() => ({
  ...emptyResults, context: null as { side: Side; root: string; mode: SearchMode } | null,
  indexed: false, root: "", mode: "files" as SearchMode, name: "", nameRegex: false,
  content: "", contentRegex: false, caseSensitive: false,
  ignore: "node_modules .git target .cache dist", minSizeMb: 10,
}));

/** Session memory survives LazyFeature unmounts; it is never written to disk. */
export function prepareSearch(side: Side, root: string, mode: SearchMode) {
  const previous = useSearchSession.getState().context;
  if (previous?.side === side && previous.root === root && previous.mode === mode) return;
  useSearchSession.setState({ ...emptyResults, root, mode, indexed: false, context: { side, root, mode } });
}
export function clearSearchResults() { useSearchSession.setState(emptyResults); }

type Session = ReturnType<typeof useSearchSession.getState>;
export function useSearchField<K extends keyof Session>(key: K) {
  const value = useSearchSession(s => s[key]);
  const set = useCallback((next: Session[K] | ((previous: Session[K]) => Session[K])) => {
    useSearchSession.setState(s => ({ [key]: typeof next === "function" ? next(s[key]) : next } as Pick<Session, K>));
  }, [key]);
  return [value, set] as const;
}
