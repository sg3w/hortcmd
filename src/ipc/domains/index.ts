import type { IndexInfo } from "../bindings/IndexInfo";
import type { IndexPolicy } from "../bindings/IndexPolicy";
import type { IndexQuery } from "../bindings/IndexQuery";
import type { IndexPage } from "../bindings/IndexPage";
import { hasTauri, invoke } from "../invoke";
export const indexList = (): Promise<IndexInfo[]> => hasTauri() ? invoke("index_list", {}) : Promise.resolve([]);
export function indexCreate(root: string, policy: IndexPolicy, password: string | null): Promise<IndexInfo> {
  return invoke("index_create", { root, policy, password });
}
export const indexUnlock = (id: string, password: string): Promise<IndexInfo> => invoke("index_unlock", { id, password });
export const indexStep = (id: string, token: string): Promise<IndexInfo> => invoke("index_step", { id, token });
export const indexCancel = (token: string): Promise<void> => invoke("index_cancel", { token });
export const indexControl = (id: string, action: "pause" | "resume" | "rebuild" | "delete"): Promise<void> => invoke("index_control", { id, action });
export const indexQuery = (id: string, query: IndexQuery, token: string): Promise<IndexPage> => invoke("index_query", { id, query, token });
