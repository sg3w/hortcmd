import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { panelOf, usePanes } from "../panesStore";
import { useOps } from "../opsStore";
import { translate, useSettings } from "../settingsStore";
import type { AppError } from "@/ipc/bindings";

vi.mock("@/ipc/invoke", async original => ({
  ...await original<typeof import("@/ipc/invoke")>(), hasTauri: () => true,
}));
async function fail(error: unknown) {
  useSettings.setState({ gitEnabled: false });
  useTauri({ commands: {
    list_dir: ({ path }) => ({ path, entries: [] }),
    list_archive: () => { throw error; },
    location_capabilities: () => ({ scheme: "local", store_id: "local", connected: true, caps: [] }),
  } });
  await usePanes.getState().loadDir("left", "/before");
  await usePanes.getState().loadArchive("left", "/damaged.zip", "");
}
const structured = (code: string): AppError => ({ version: 1, code, id: "archive-reference", params: {},
  op: "list_archive", provider: "archive", retryable: false, action: "report", detail: "private diagnostic" });

it.each(["en", "de"] as const)("presents a translated archive failure in %s and retains its structured state", async language => {
  useSettings.setState({ language });
  const error = structured("archive.corrupt");
  await fail(error);
  expect(useOps.getState().confirm?.message).toBe(translate("error.archive.corrupt"));
  expect(useOps.getState().prompt).toBeNull();
  expect(panelOf(usePanes.getState(), "left")).toMatchObject({ path: "/before", archive: null,
    loadError: error, history: [{ path: "/before", archive: null }], historyIndex: 0 });
});

it("includes the reference for an unknown error without showing diagnostic detail", async () => {
  await fail(structured("archive.futureFailure"));
  const message = useOps.getState().confirm!.message;
  expect(message).toContain("archive.futureFailure");
  expect(message).toContain("archive-reference");
  expect(message).not.toContain("private diagnostic");
  expect(message).not.toContain("[object Object]");
});

it("retains readable transport errors", async () => {
  await fail(new Error("transport closed"));
  expect(useOps.getState().confirm?.message).toBe("Error: transport closed");
  expect(useOps.getState().prompt).toBeNull();
});
