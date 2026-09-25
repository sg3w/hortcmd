import { expect, it } from "vitest";
import { waitFor } from "@testing-library/react";
import { useTauri } from "@/test/tauri";
import { file } from "@/test/fixtures";
import { usePanes } from "@/store/panesStore";
import { useCaps } from "@/store/capsStore";
import { useOps } from "@/store/opsStore";
import { translate, useSettings } from "@/store/settingsStore";
import { describeErrorFull } from "@/store/errorText";
import { runEdit, runMkdir } from "./fileOps";
import { openEntry } from "./navigate";

it.each([runEdit, (side: "left" | "right") => openEntry(side, 0)])("reports failed default-program launches", async action => {
  const error = { code: "fs.permission", params: {}, id: "launch-test", v: 1 };
  const runtime = useTauri({ commands: { open_path: () => Promise.reject(error) } });
  useSettings.setState({ language: "en" });
  useCaps.setState({ known: { local: ["systemOpen"] } });
  const entries = [file("notes.txt")];
  usePanes.setState(s => ({ left: { ...s.left, activeTab: 0, tabs: [{ ...s.left.tabs[0], path: "/source", archive: null, entries, raw: entries, cursor: 0 }] } }));
  action("left");
  await waitFor(() => expect(useOps.getState().confirm?.message).toBe(describeErrorFull(error)));
  expect(useOps.getState().confirm).toMatchObject({ title: translate("open.error"), danger: true });
  expect(runtime.calls.filter(call => call.cmd === "open_path")).toHaveLength(1);
});

it("reports mkdir failures and releases the busy state", async () => {
  const error = { code: "fs.exists", params: {}, id: "mkdir-test", v: 1 };
  const runtime = useTauri({ commands: { make_dir: () => Promise.reject(error) } });
  useCaps.setState({ known: { local: ["create"] } });
  usePanes.setState(s => ({ left: { ...s.left, activeTab: 0, tabs: [{ ...s.left.tabs[0], path: "/source" }] } }));
  runMkdir("left");
  const prompt = useOps.getState().prompt;
  expect(prompt).not.toBeNull();
  useOps.getState().closeDialogs();
  prompt!.onSubmit(" taken ", false);
  await waitFor(() => expect(useOps.getState().confirm?.message).toBe(describeErrorFull(error)));
  expect(useOps.getState().busy).toBe(false);
  expect(runtime.calls.filter(call => call.cmd === "make_dir")).toEqual([{ cmd: "make_dir", args: { path: "/source/taken" } }]);
});
