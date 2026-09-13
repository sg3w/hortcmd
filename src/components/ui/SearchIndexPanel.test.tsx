import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { useSettings, translate } from "@/store/settingsStore";
import type { IndexInfo } from "@/ipc/bindings/IndexInfo";
import { SearchIndexPanel } from "./SearchIndexPanel";
vi.mock("@/ipc/invoke", async importOriginal => ({
  ...await importOriginal<typeof import("@/ipc/invoke")>(), hasTauri: true,
}));
const info: IndexInfo = { id: "0-abcd", root: "/approved", state: "paused", entries: 2, bytes: 500, pending: 0, unreadable: 0, persistent: false,
  policy: { content: false, hidden: false, offline: false, allowBattery: false, ignore: [], maxContentBytes: 1048576 } };
it("creates no scope until approval and defaults to metadata in memory", async () => {
  const list: IndexInfo[] = [];
  const runtime = useTauri({ commands: { index_list: () => list, index_create: () => { list.push(info); return info; }, index_cancel: () => {} } });
  useSettings.setState({ language: "en" });
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await waitFor(() => expect(runtime.calls.some(c => c.cmd === "index_list")).toBe(true));
  expect(runtime.calls.some(c => c.cmd === "index_create")).toBe(false);
  await userEvent.click(screen.getByRole("button", { name: translate("index.approve") }));
  await waitFor(() => expect(runtime.calls.find(c => c.cmd === "index_create")?.args).toMatchObject({ root: "/approved", password: null, policy: { content: false, hidden: false, offline: false, allowBattery: false } }));
});
it("labels offline snapshots and prevents revealing disconnected results", async () => {
  useSettings.setState({ language: "en" });
  useTauri({ commands: { index_list: () => [info], index_query: () => ({ hits: [{ id: "hit", relative: "old.txt", address: null, directory: false, size: 3, modified: 0, provenance: "metadata" }], next: null, revision: 1, freshness: "offline", source: "application_index", fallback: false }), index_cancel: () => {} } });
  const reveal = vi.fn(); render(<SearchIndexPanel initialRoot="/approved" onReveal={reveal} />);
  await screen.findByRole("option", { name: /\/approved/ });
  await userEvent.selectOptions(screen.getByRole("combobox", { name: translate("index.scopes") }), info.id);
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  expect(await screen.findByRole("button", { name: "old.txt" })).toBeDisabled();
  expect(screen.getByText(/Offline snapshot/)).toBeVisible(); expect(reveal).not.toHaveBeenCalled();
});
it("cancels an unfinished query when the index view closes", async () => {
  useSettings.setState({ language: "en" });
  const runtime = useTauri({ commands: { index_list: () => [info], index_query: () => new Promise(() => {}), index_cancel: () => {} } });
  const view = render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await screen.findByRole("option", { name: /\/approved/ });
  await userEvent.selectOptions(screen.getByRole("combobox", { name: translate("index.scopes") }), info.id);
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  await waitFor(() => expect(runtime.calls.some(c => c.cmd === "index_query")).toBe(true));
  view.unmount();
  await waitFor(() => expect(runtime.calls.find(c => c.cmd === "index_cancel")?.args.token).toBe(runtime.calls.find(c => c.cmd === "index_query")?.args.token));
});
