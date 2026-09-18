import { act, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import type { OperationPlan } from "@/ipc/bindings/OperationPlan";
const transfer={id:"plugin-copy",instanceId:"instance",pluginName:"Tools",kind:"copy",sources:["/source"],destination:"/target",copyLimit:128};
it("reviews plugin transfers before execution and retains the configured limit",async()=>{
  const harness=useTauri({commands:{plugin_ui_poll:()=>({events:[],transfers:[transfer]}),plugin_transfer_start:()=>{}}});
  vi.resetModules();const {startPluginActions}=await import("./actions");const {useOps}=await import("@/store/opsStore");const {useTransfers}=await import("@/store/transfersStore");
  const stop=startPluginActions();
  await waitFor(()=>expect(useOps.getState().confirm).not.toBeNull());
  expect(harness.calls.some(c=>c.cmd === "plugin_transfer_start")).toBe(false);
  const settings=useOps.getState().confirm!.preflight!.settings;
  expect(settings.limit).toBe(128*1024);
  act(()=>useOps.getState().confirmDialog({digest:"reviewed",settings} as OperationPlan));
  await waitFor(()=>expect(harness.calls.find(c=>c.cmd === "plugin_transfer_start")?.args).toEqual({id:"plugin-copy",settings,review:"reviewed"}));
  expect(useTransfers.getState().transfers[0].id).toBe("plugin-copy");stop();
});
it("cancels a declined request without starting a transfer",async()=>{
  const harness=useTauri({commands:{plugin_ui_poll:()=>({events:[],transfers:[transfer]}),plugin_transfer_cancel:()=>{}}});
  vi.resetModules();const {startPluginActions}=await import("./actions");const {useOps}=await import("@/store/opsStore");
  const stop=startPluginActions();await waitFor(()=>expect(useOps.getState().confirm).not.toBeNull());
  act(()=>useOps.getState().closeDialogs());
  await waitFor(()=>expect(harness.calls.some(c=>c.cmd === "plugin_transfer_cancel")).toBe(true));
  expect(harness.calls.some(c=>c.cmd === "plugin_transfer_start")).toBe(false);stop();
});
