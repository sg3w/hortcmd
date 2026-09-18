import { create } from "zustand";
import type { IndexInfo } from "@/ipc/bindings/IndexInfo";
import type { IndexPage } from "@/ipc/bindings/IndexPage";

export type IndexQueryStatus = "idle" | "running" | "complete" | "cancelled" | "error";
export interface IndexSearchSnapshot {
  initialRoot: string;
  root: string;
  selected: string;
  content: boolean;
  hidden: boolean;
  offline: boolean;
  battery: boolean;
  ignore: string;
  persistent: boolean;
  name: string;
  text: string;
  path: string;
  sensitive: boolean;
  kind: string;
  minimum: string;
  maximum: string;
  modified: string;
  page: IndexPage | null;
  error: string;
  scopeKey: string | null;
  status: IndexQueryStatus;
}

// One ephemeral session, with no persistence middleware or credential field.
export const useIndexSearchSession = create(() => ({ snapshot: null as IndexSearchSnapshot | null }));
export function readIndexSearchSession(initialRoot: string): IndexSearchSnapshot {
  const saved = useIndexSearchSession.getState().snapshot;
  if (saved?.initialRoot === initialRoot) return saved;
  return {
    initialRoot, root: initialRoot, selected: "", content: false, hidden: false,
    offline: false, battery: false, ignore: "node_modules target dist .cache", persistent: false,
    name: "", text: "", path: "", sensitive: false, kind: "any", minimum: "", maximum: "", modified: "",
    page: null, error: "", scopeKey: null, status: "idle",
  };
}
export function saveIndexSearchSession(snapshot: IndexSearchSnapshot) {
  useIndexSearchSession.setState({ snapshot });
}
/** Presentation identity only; the backend remains the authority for every query. */
export function indexScopeKey(scope: IndexInfo | undefined): string | null {
  if (!scope || scope.state === "locked" || scope.root === null || scope.policy === null
    || (scope.state === "offline" && !scope.policy.offline)) return null;
  return JSON.stringify([scope.id, scope.root, scope.policy, scope.state === "offline"]);
}
