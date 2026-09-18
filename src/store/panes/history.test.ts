import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { panelOf, usePanes } from "../panesStore";
import { useOps } from "../opsStore";
import { useSettings } from "../settingsStore";
import { useSession } from "../sessionStore";
import { openEntry, openOpposite } from "@/features/commander/navigate";
import { file } from "@/test/fixtures";
import { recordNavigation } from "./history";

vi.mock("@/ipc/invoke", async original => ({
  ...await original<typeof import("@/ipc/invoke")>(), hasTauri: true,
}));
const tab = () => panelOf(usePanes.getState(), "left");
const folder = (path: string) => ({ path, archive: null });
const archive = (path: string, container = "/pack.zip") => ({ path, archive: container });
function setup() {
  useSettings.setState({ gitEnabled: false });
  return useTauri({ commands: {
    list_dir: ({ path }) => ({ path, entries: [file("pack.zip")] }),
    list_archive: ({ inner }) => ({ path: inner, entries: [file("nested", { is_dir: true })] }),
    location_capabilities: () => ({ scheme: "local", store_id: "local", connected: true, caps: ["browse"] }),
    home_dir: () => "/home",
  } });
}
async function back(expected: { path: string; archive: string | null }) {
  usePanes.getState().goBack("left");
  await vi.waitFor(() => expect(tab()).toMatchObject({ ...expected, loading: false }));
}
async function forward(expected: { path: string; archive: string | null }) {
  usePanes.getState().goForward("left");
  await vi.waitFor(() => expect(tab()).toMatchObject({ ...expected, loading: false }));
}

it("records normal archive opening, nested places and exiting; revisits without appending", async () => {
  setup();
  await usePanes.getState().loadDir("left", "/");
  openEntry("left", tab().entries.findIndex(e => e.name === "pack.zip"));
  await vi.waitFor(() => expect(tab().archive).toBe("/pack.zip"));
  openEntry("left", tab().entries.findIndex(e => e.name === "nested"));
  await vi.waitFor(() => expect(tab().path).toBe("/nested"));
  await usePanes.getState().loadDir("left", "sftp://user@host/remote");
  const history = [folder("/"), archive(""), archive("/nested"), folder("sftp://user@host/remote")];
  expect(tab().history).toEqual(history);
  await back(archive("/nested")); await back(archive("")); await back(folder("/"));
  await forward(archive("")); await forward(archive("/nested")); await forward(folder("sftp://user@host/remote"));
  expect(tab().history).toEqual(history);
  expect(tab().historyIndex).toBe(3);
});

it("keeps different containers distinct and truncates the forward branch after a new destination", async () => {
  setup();
  await usePanes.getState().loadArchive("left", "/first.zip", "same");
  await usePanes.getState().loadArchive("left", "/second.zip", "same");
  await usePanes.getState().loadArchive("left", "/second.zip", "same");
  expect(tab().history).toEqual([archive("same", "/first.zip"), archive("same", "/second.zip")]);
  await back(archive("same", "/first.zip"));
  await usePanes.getState().loadDir("left", "/new");
  expect(tab().history).toEqual([archive("same", "/first.zip"), folder("/new")]);
});

it("records opposite-pane archive opening in the destination history", async () => {
  setup();
  await usePanes.getState().loadDir("left", "/before");
  await usePanes.getState().loadArchive("right", "/pack.zip", "nested");
  const source = panelOf(usePanes.getState(), "right");
  await openOpposite("right");
  await back(folder("/before")); await forward(archive("nested"));
  expect(panelOf(usePanes.getState(), "right")).toEqual(source);
  expect(tab().history).toEqual([folder("/before"), archive("nested")]);
});

it("keeps the committed history position on failure and ignores a late back response", async () => {
  const runtime = setup();
  await usePanes.getState().loadArchive("left", "/pack.zip", "nested");
  await usePanes.getState().loadDir("left", "/current");
  runtime.setCommand("list_archive", () => { throw new Error("unavailable"); });
  usePanes.getState().goBack("left");
  await vi.waitFor(() => expect(tab().loading).toBe(false));
  expect(tab()).toMatchObject({ path: "/current", archive: null, historyIndex: 1 });
  let resolve!: (value: { path: string; entries: never[] }) => void;
  runtime.setCommand("list_archive", () => new Promise(yes => { resolve = yes; }));
  usePanes.getState().goBack("left");
  await vi.waitFor(() => expect(resolve).toBeDefined());
  expect(tab().historyIndex).toBe(1);
  await usePanes.getState().loadDir("left", "/newer");
  resolve({ path: "nested", entries: [] });
  await new Promise(yes => setTimeout(yes, 0));
  expect(tab()).toMatchObject({ path: "/newer", archive: null, historyIndex: 2 });
  expect(tab().history).toEqual([archive("nested"), folder("/current"), folder("/newer")]);
});

it("commits a password-protected history destination only after successful retry", async () => {
  const runtime = setup();
  await usePanes.getState().loadArchive("left", "/protected-history.zip", "inside");
  await usePanes.getState().loadDir("left", "/current");
  runtime.setCommand("list_archive", ({ inner, password }) => {
    if (password !== "history secret") throw { version: 1, code: "archive.passwordRequired", id: "history-password" };
    return { path: inner, entries: [] };
  });
  usePanes.getState().goBack("left");
  await vi.waitFor(() => expect(useOps.getState().prompt).not.toBeNull());
  expect(tab()).toMatchObject({ path: "/current", archive: null, historyIndex: 1 });
  useOps.getState().prompt!.onSubmit("history secret", false);
  await vi.waitFor(() => expect(tab()).toMatchObject({ path: "inside", archive: "/protected-history.zip", historyIndex: 0 }));
  expect(tab().history).toHaveLength(2);
  expect(JSON.stringify(tab().history)).not.toContain("history secret");
  await forward(folder("/current"));
});

it("migrates legacy string histories when navigating and keeps saved layout restoration compatible", async () => {
  setup();
  await usePanes.getState().loadDir("left", "/current");
  usePanes.setState(s => ({ left: { ...s.left, tabs: [{ ...tab(), history: ["/old", "/current"], historyIndex: 1 }] } }));
  await back(folder("/old"));
  expect(tab().history).toEqual([folder("/old"), folder("/current")]);
  await usePanes.getState().loadArchive("left", "/folder/pack.zip", "nested");
  useSession.getState().capture();
  const saved = useSession.getState().session!;
  expect(saved.left.tabs).toEqual([{ path: "/folder" }]);
  await usePanes.getState().restoreSession(saved);
  expect(tab().history).toEqual([folder("/folder")]);
  expect(tab().archive).toBeNull();
});

it("normalizes legacy entries without appending on refresh", () => {
  expect(recordNavigation({ history: ["/old"], historyIndex: 0 }, folder("/old"), false))
    .toEqual({ history: [folder("/old")], historyIndex: 0 });
});

it("records leaving the archive through its parent row", async () => {
  setup();
  await usePanes.getState().loadArchive("left", "/folder/pack.zip", "");
  openEntry("left", tab().entries.findIndex(e => e.parent));
  await vi.waitFor(() => expect(tab()).toMatchObject({ path: "/folder", archive: null, loading: false }));
  expect(tab().history).toEqual([archive("", "/folder/pack.zip"), folder("/folder")]);
  await back(archive("", "/folder/pack.zip"));
});

it("retains history when a directory destination cannot be loaded", async () => {
  const runtime = setup();
  await usePanes.getState().loadDir("left", "/missing");
  await usePanes.getState().loadArchive("left", "/pack.zip", "nested");
  runtime.setCommand("list_dir", () => { throw new Error("directory missing"); });
  usePanes.getState().goBack("left");
  await vi.waitFor(() => expect(tab().loading).toBe(false));
  expect(tab()).toMatchObject({ path: "nested", archive: "/pack.zip", historyIndex: 1 });
  expect(tab().history).toEqual([folder("/missing"), archive("nested")]);
});
