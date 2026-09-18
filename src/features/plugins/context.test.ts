import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
it("publishes selection, cursor, side, limit and copy handoff through typed IPC", async () => {
  const harness=useTauri({commands:{plugin_workspace:()=>{}}}); vi.resetModules();
  const {usePanes}=await import("@/store/panesStore"); const {newTab}=await import("@/store/panes/state");
  const {useSettings}=await import("@/store/settingsStore"); const {snapshot,publishContext}=await import("./context");
  const {file}=await import("@/test/fixtures");
  usePanes.setState(s=>({active:"right",left:{...s.left,tabs:[newTab("/left")]},right:{...s.right,tabs:[{...newTab("/right"),entries:[file("source.php")],selected:new Set(["source.php"])}]}}));
  useSettings.setState({speedLimit:512});
  const operation={sources:["/right/source.php"],destination:"/left"}; await publishContext(operation);
  expect(snapshot(operation)?.right).toMatchObject({path:"/right",folderPath:"/right",filePath:"/right/source.php",selection:["/right/source.php"],side:"right",copyLimit:512,copyOperation:operation});
  expect(harness.calls[0].args.workspace).toEqual(snapshot(operation));
});
