import { expect, it } from "vitest";
import { waitFor } from "@testing-library/react";
import { useTauri } from "@/test/tauri";
import { dir, file } from "@/test/fixtures";
import { usePanes } from "@/store/panesStore";
import { useCaps } from "@/store/capsStore";
import { useOps } from "@/store/opsStore";
import { translate, useSettings } from "@/store/settingsStore";
import { describeErrorFull } from "@/store/errorText";
import { runEdit, runMkdir, runPack, runRename } from "./fileOps";
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

it("submits a case-only rename instead of reporting it as a conflict", async () => {
  const runtime = useTauri({ commands: { rename_entry: () => Promise.resolve(null) } });
  useCaps.setState({ known: { local: ["move", "copyTo"] } });
  const entries = [file("Readme.txt"), file("notes.txt")];
  usePanes.setState(s => ({ left: { ...s.left, activeTab: 0, tabs: [{ ...s.left.tabs[0], path: "/source", archive: null, entries, raw: entries, cursor: 0 }] } }));

  runRename("left");
  const prompt = useOps.getState().prompt;
  expect(prompt).not.toBeNull();
  useOps.getState().closeDialogs();
  prompt!.onSubmit("readme.txt", false);

  await waitFor(() =>
    expect(runtime.calls.filter(call => call.cmd === "rename_entry")).toEqual([
      { cmd: "rename_entry", args: { from: "/source/Readme.txt", to: "/source/readme.txt" } },
    ]),
  );
  expect(useOps.getState().confirm).toBeNull();
});

it("still reports a name taken by another entry, whatever its spelling", async () => {
  const runtime = useTauri({ commands: { rename_entry: () => Promise.resolve(null) } });
  useCaps.setState({ known: { local: ["move", "copyTo"] } });
  useSettings.setState({ language: "en" });
  const entries = [file("Readme.txt"), file("notes.txt")];
  usePanes.setState(s => ({ left: { ...s.left, activeTab: 0, tabs: [{ ...s.left.tabs[0], path: "/source", archive: null, entries, raw: entries, cursor: 1 }] } }));

  runRename("left");
  const prompt = useOps.getState().prompt;
  useOps.getState().closeDialogs();
  prompt!.onSubmit("README.TXT", false);

  await waitFor(() => expect(useOps.getState().confirm?.message).toContain("README.TXT"));
  expect(runtime.calls.filter(call => call.cmd === "rename_entry")).toHaveLength(0);
});

it("proposes the folder name with a timestamp when packing, preselecting all but the extension", async () => {
  const runtime = useTauri({ commands: { create_archive: () => Promise.resolve(null) } });
  useCaps.setState({ known: { local: ["archivePackFrom", "archivePackTo"] } });
  const entries = [file("notes.txt"), dir("Fotos"), dir("Videos")];
  usePanes.setState(s => ({
    left: { ...s.left, activeTab: 0, tabs: [{ ...s.left.tabs[0], path: "/source", archive: null, entries, raw: entries, cursor: 0, selected: new Set(["notes.txt", "Videos", "Fotos"]) }] },
    right: { ...s.right, activeTab: 0, tabs: [{ ...s.right.tabs[0], path: "/target", archive: null }] },
  }));

  runPack("left");
  const prompt = useOps.getState().prompt;
  expect(prompt).not.toBeNull();
  // Folder wins over the file, and the list order decides between the two
  // folders — not the order they were marked in.
  expect(prompt!.initial).toMatch(/^Fotos_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}\.zip$/);
  expect(prompt!.selectRange).toEqual([0, prompt!.initial.length - 4]);

  useOps.getState().closeDialogs();
  prompt!.onSubmit(prompt!.initial, false);

  await waitFor(() =>
    expect(runtime.calls.filter(call => call.cmd === "create_archive")).toMatchObject([
      { cmd: "create_archive", args: { sources: expect.arrayContaining(["/source/Fotos"]), destZip: `/target/${prompt!.initial}` } },
    ]),
  );
});
