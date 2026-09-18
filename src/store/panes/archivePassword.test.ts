import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { usePanes, panelOf } from "../panesStore";
import { useOps } from "../opsStore";
import { translate } from "../settingsStore";
import type { AppError } from "@/ipc/bindings";

const error = (code: string): AppError => ({ version: 1, code, id: "archive-test", params: {},
  op: "list_archive", provider: null, retryable: true, action: null, detail: null });
vi.mock("@/ipc/invoke", async original => ({
  ...await original<typeof import("@/ipc/invoke")>(), hasTauri: true,
}));

it.each(["archive.passwordRequired", "archive.passwordWrong"])("prompts and retries for structured %s", async code => {
  const archive = `/password-${code}.zip`;
  const failure = error(code);
  const runtime = useTauri({ commands: { list_archive: ({ inner, password }) => {
    if (password !== "test secret") throw failure;
    return { path: inner, entries: [] };
  } } });
  await usePanes.getState().loadArchive("left", archive, "nested");
  expect(panelOf(usePanes.getState(), "left").loadError).toEqual(failure);
  expect(useOps.getState().confirm).toBeNull();
  const prompt = useOps.getState().prompt!;
  expect(prompt.password).toBe(true);
  expect(prompt.label).toBe(translate(code === "archive.passwordWrong" ? "archive.password.retry" : "archive.password.label"));
  prompt.onSubmit("test secret", false);
  await vi.waitFor(() => expect(panelOf(usePanes.getState(), "left")).toMatchObject({ archive, path: "nested", loadError: null }));
  expect(runtime.calls.filter(c => c.cmd === "list_archive").slice(-1)[0]?.args).toMatchObject({ password: "test secret" });
});

it("does not retry an old password prompt after another navigation", async () => {
  const runtime = useTauri({ commands: { list_archive: () => { throw error("archive.passwordRequired"); },
    list_dir: ({ path }) => ({ path, entries: [] }),
    location_capabilities: () => ({ scheme: "local", store_id: "local", connected: true, caps: [] }) } });
  await usePanes.getState().loadArchive("left", "/old-password.zip", "");
  const prompt = useOps.getState().prompt!;
  await usePanes.getState().loadDir("left", "/chosen");
  prompt.onSubmit("late secret", false);
  expect(runtime.calls.filter(c => c.cmd === "list_archive")).toHaveLength(1);
  expect(panelOf(usePanes.getState(), "left").path).toBe("/chosen");
});

it("does not display a password prompt from a superseded archive request", async () => {
  let reject!: (value: unknown) => void;
  useTauri({ commands: { list_archive: () => new Promise((_yes, no) => { reject = no; }),
    list_dir: ({ path }) => ({ path, entries: [] }),
    location_capabilities: () => ({ scheme: "local", store_id: "local", connected: true, caps: [] }) } });
  const old = usePanes.getState().loadArchive("left", "/late-password.zip", "");
  await vi.waitFor(() => expect(reject).toBeDefined());
  await usePanes.getState().loadDir("left", "/chosen");
  reject(error("archive.passwordRequired")); await old;
  expect(useOps.getState().prompt).toBeNull();
  expect(panelOf(usePanes.getState(), "left")).toMatchObject({ path: "/chosen", loadError: null });
});
