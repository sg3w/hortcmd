import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { useSettings, translate } from "@/store/settingsStore";
import type { IndexInfo } from "@/ipc/bindings/IndexInfo";
import { useIndexSearchSession } from "@/store/indexSearchSession";
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

function pending<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const result = (relative = "remembered.txt") => ({ hits: [{ id: relative, relative, address: `/approved/${relative}`, directory: false, size: 3, modified: 0, provenance: "metadata" }], next: null, revision: 1, freshness: "snapshot", source: "application_index", fallback: false });
async function chooseScope(id = info.id) {
  await screen.findByRole("option", { name: /\/approved/ });
  await userEvent.selectOptions(screen.getByRole("combobox", { name: translate("index.scopes") }), id);
  await waitFor(() => expect(screen.getByRole("button", { name: translate("search.run") })).toBeEnabled());
}

it("restores parameters and results only after scope validation, without rerunning the query", async () => {
  useSettings.setState({ language: "en" });
  const query = vi.fn(() => result());
  const validation = pending<IndexInfo[]>();
  let validateAgain = false;
  useTauri({ commands: { index_list: () => validateAgain ? validation.promise : [info], index_query: query, index_cancel: () => {} } });
  const view = render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await chooseScope();
  await userEvent.type(screen.getByRole("textbox", { name: translate("search.name") }), "*.txt");
  await userEvent.type(screen.getByRole("textbox", { name: translate("index.path") }), "reports");
  await userEvent.click(screen.getByRole("checkbox", { name: translate("search.case") }));
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  await screen.findByRole("button", { name: "remembered.txt" });
  view.unmount(); validateAgain = true;
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  expect(screen.queryByRole("button", { name: "remembered.txt" })).not.toBeInTheDocument();
  await act(async () => validation.resolve([info]));
  await screen.findByRole("button", { name: "remembered.txt" });
  expect(screen.getByRole("textbox", { name: translate("search.name") })).toHaveValue("*.txt");
  expect(screen.getByRole("textbox", { name: translate("index.path") })).toHaveValue("reports");
  expect(screen.getByRole("checkbox", { name: translate("search.case") })).toBeChecked();
  expect(query).toHaveBeenCalledTimes(1);
});

it.each(["locked", "removed", "changed-root", "changed-policy"])("invalidates retained results when the scope is %s while closed", async change => {
  useSettings.setState({ language: "en" });
  let list: IndexInfo[] = [info];
  const query = vi.fn(() => result());
  useTauri({ commands: { index_list: () => list, index_query: query, index_cancel: () => {} } });
  const view = render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await chooseScope();
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  await screen.findByRole("button", { name: "remembered.txt" });
  view.unmount();
  list = change === "removed" ? [] : [{ ...info,
    ...(change === "locked" ? { state: "locked", root: null, policy: null } : {}),
    ...(change === "changed-root" ? { root: "/replacement" } : {}),
    ...(change === "changed-policy" ? { policy: { ...info.policy!, content: true } } : {}),
  }];
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await waitFor(() => expect(useIndexSearchSession.getState().snapshot?.page).toBeNull());
  expect(screen.queryByRole("button", { name: "remembered.txt" })).not.toBeInTheDocument();
  expect(query).toHaveBeenCalledTimes(1);
});

it("refuses a query response if scope validation reports a lock before publication", async () => {
  useSettings.setState({ language: "en" });
  const response = pending<ReturnType<typeof result>>();
  let list: IndexInfo[] = [info];
  useTauri({ commands: { index_list: () => list, index_query: () => response.promise, index_cancel: () => {} } });
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await chooseScope();
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  list = [{ ...info, state: "locked", root: null, policy: null }];
  await act(async () => response.resolve(result("private.txt")));
  await screen.findByRole("button", { name: translate("index.unlock") });
  expect(screen.queryByRole("button", { name: "private.txt" })).not.toBeInTheDocument();
  expect(useIndexSearchSession.getState().snapshot?.page).toBeNull();
});

it("keeps cancellation after remount and discards a late query response", async () => {
  useSettings.setState({ language: "en" });
  const response = pending<ReturnType<typeof result>>();
  const query = vi.fn(() => response.promise);
  const runtime = useTauri({ commands: { index_list: () => [info], index_query: query, index_cancel: () => {} } });
  const view = render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await chooseScope();
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  view.unmount();
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await screen.findByText(translate("search.cancelled"));
  await act(async () => response.resolve(result("late.txt")));
  expect(screen.queryByRole("button", { name: "late.txt" })).not.toBeInTheDocument();
  expect(query).toHaveBeenCalledTimes(1);
  expect(runtime.calls.some(call => call.cmd === "index_cancel")).toBe(true);
  expect(useIndexSearchSession.getState().snapshot?.status).toBe("cancelled");
});

it("does not let a late failure from the previous scope overwrite a newer query", async () => {
  useSettings.setState({ language: "en" });
  const old = pending<ReturnType<typeof result>>();
  const newer = pending<ReturnType<typeof result>>();
  const second = { ...info, id: "second", root: "/approved/second" };
  useTauri({ commands: { index_list: () => [info, second], index_query: ({ id }) => id === info.id ? old.promise : newer.promise, index_cancel: () => {} } });
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await waitFor(() => expect(screen.getAllByRole("option", { name: /\/approved/ })).toHaveLength(2));
  await userEvent.selectOptions(screen.getByRole("combobox", { name: translate("index.scopes") }), info.id);
  await waitFor(() => expect(screen.getByRole("button", { name: translate("search.run") })).toBeEnabled());
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  await userEvent.selectOptions(screen.getByRole("combobox", { name: translate("index.scopes") }), second.id);
  await waitFor(() => expect(screen.getByRole("button", { name: translate("search.run") })).toBeEnabled());
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  await act(async () => old.reject(new Error("late old failure")));
  expect(screen.queryByText(/late old failure/)).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: translate("search.run") })).toBeDisabled();
  await act(async () => newer.resolve(result("new.txt")));
  await screen.findByRole("button", { name: "new.txt" });
});

it("keeps creation preferences in memory but never the password", async () => {
  useSettings.setState({ language: "en" });
  useTauri({ commands: { index_list: () => [] } });
  const storedBefore = localStorage.length;
  const view = render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await userEvent.click(screen.getByRole("checkbox", { name: translate("index.persist") }));
  await userEvent.type(screen.getByLabelText(translate("index.password")), "secret-only-in-component");
  await userEvent.click(screen.getByRole("checkbox", { name: translate("index.hidden") }));
  view.unmount();
  expect(JSON.stringify(useIndexSearchSession.getState())).not.toContain("secret-only-in-component");
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  expect(screen.getByLabelText(translate("index.password"))).toHaveValue("");
  expect(screen.getByRole("checkbox", { name: translate("index.hidden") })).toBeChecked();
  expect(localStorage.length).toBe(storedBefore);
});

it("stops immediately and does not publish the stopped response", async () => {
  useSettings.setState({ language: "en" });
  const response = pending<ReturnType<typeof result>>();
  useTauri({ commands: { index_list: () => [info], index_query: () => response.promise, index_cancel: () => {} } });
  const view = render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await chooseScope();
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  await userEvent.click(screen.getByRole("button", { name: translate("search.stop") }));
  expect(screen.getByRole("button", { name: translate("search.run") })).toBeEnabled();
  await act(async () => response.resolve(result("stopped.txt")));
  expect(screen.queryByRole("button", { name: "stopped.txt" })).not.toBeInTheDocument();
  view.unmount();
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await screen.findByText(translate("search.cancelled"));
});

it("does not select a late-created scope after the previous panel has closed", async () => {
  useSettings.setState({ language: "en" });
  const created = pending<IndexInfo>();
  useTauri({ commands: { index_list: () => [], index_create: () => created.promise } });
  const view = render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await userEvent.click(screen.getByRole("button", { name: translate("index.approve") }));
  view.unmount();
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await act(async () => created.resolve(info));
  expect(screen.getByRole("combobox", { name: translate("index.scopes") })).toHaveValue("");
  expect(useIndexSearchSession.getState().snapshot?.selected).toBe("");
});

it("starts a fresh session when the launch folder changes", async () => {
  useSettings.setState({ language: "en" });
  useTauri({ commands: { index_list: () => [info], index_query: () => result(), index_cancel: () => {} } });
  const view = render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await chooseScope();
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  await screen.findByRole("button", { name: "remembered.txt" });
  view.rerender(<SearchIndexPanel initialRoot="/different" onReveal={vi.fn()} />);
  expect(screen.getByRole("textbox", { name: translate("search.root") })).toHaveValue("/different");
  expect(screen.queryByRole("button", { name: "remembered.txt" })).not.toBeInTheDocument();
  expect(useIndexSearchSession.getState().snapshot?.selected).toBe("");
});

it("retains a query failure for the same validated scope without retrying", async () => {
  useSettings.setState({ language: "en" });
  const query = vi.fn(() => { throw new Error("Fixture query failure"); });
  useTauri({ commands: { index_list: () => [info], index_query: query, index_cancel: () => {} } });
  const view = render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await chooseScope();
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  await screen.findByRole("alert");
  view.unmount();
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(await screen.findByRole("alert")).toHaveTextContent("Fixture query failure");
  expect(query).toHaveBeenCalledTimes(1);
});

it("unlocking a previously locked scope never revives its discarded results or content query", async () => {
  useSettings.setState({ language: "en" });
  const contentScope = { ...info, policy: { ...info.policy!, content: true } };
  let list: IndexInfo[] = [contentScope];
  const query = vi.fn(() => result("private.txt"));
  useTauri({ commands: {
    index_list: () => list, index_query: query, index_cancel: () => {},
    index_unlock: () => { list = [contentScope]; return contentScope; },
  } });
  const view = render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await chooseScope();
  await userEvent.type(screen.getByRole("textbox", { name: translate("search.content") }), "private search text");
  await userEvent.click(screen.getByRole("button", { name: translate("search.run") }));
  await screen.findByRole("button", { name: "private.txt" });
  view.unmount();
  list = [{ ...contentScope, state: "locked", root: null, policy: null }];
  render(<SearchIndexPanel initialRoot="/approved" onReveal={vi.fn()} />);
  await screen.findByRole("button", { name: translate("index.unlock") });
  expect(useIndexSearchSession.getState().snapshot?.text).toBe("");
  await userEvent.type(screen.getByLabelText(translate("index.password")), "unlock only");
  await userEvent.click(screen.getByRole("button", { name: translate("index.unlock") }));
  await waitFor(() => expect(screen.getByRole("button", { name: translate("search.run") })).toBeEnabled());
  expect(screen.queryByRole("button", { name: "private.txt" })).not.toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: translate("search.content") })).toHaveValue("");
  expect(query).toHaveBeenCalledTimes(1);
  expect(JSON.stringify(useIndexSearchSession.getState())).not.toContain("unlock only");
});
