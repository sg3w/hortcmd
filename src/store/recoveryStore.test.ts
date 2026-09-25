import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { useRecovery } from "./recoveryStore";
import { useTransfers } from "./transfersStore";
import type { RecoveryEntry } from "@/ipc/bindings/RecoveryEntry";

vi.mock("@/ipc/invoke", async original => ({ ...await original<typeof import("@/ipc/invoke")>(), hasTauri: () => true }));
const entry: RecoveryEntry = { id: "job", kind: "copy", state: "completed", order: 1,
  sources: [], destination: { kind: "local", path: "/destination" }, committed: 1, total: 1, reason: null };

it("reconciles an active transfer after its completion event was lost", async () => {
  const runtime = useTauri({ commands: { recovery_list: () => [entry] } });
  useTransfers.getState().start("copy", entry.id);
  await useRecovery.getState().refresh();
  expect(useTransfers.getState().transfers.find(t => t.id === entry.id)).toMatchObject({ done: true, cancelled: false, errors: [] });
  expect(runtime.calls.map(c => c.cmd)).toEqual(["recovery_list"]);
});

it("reattaches running work once without dispatching it again", async () => {
  const runtime = useTauri({ commands: { recovery_list: () => [{ ...entry, state: "running" }] } });
  await useRecovery.getState().refresh(true);
  await useRecovery.getState().refresh();
  expect(useTransfers.getState().transfers.filter(t => t.id === entry.id)).toHaveLength(1);
  expect(useTransfers.getState().transfers.find(t => t.id === entry.id)).toMatchObject({ done: false, minimized: true });
  expect(runtime.calls.every(c => c.cmd === "recovery_list")).toBe(true);
});
