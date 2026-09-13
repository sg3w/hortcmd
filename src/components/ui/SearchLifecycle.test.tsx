import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { useDemo } from "@/test/tauri";
import { usePanes } from "@/store/panesStore";
import { useCaps } from "@/store/capsStore";
import { useSearchDialog } from "@/store/searchStore";
import { translate, useSettings } from "@/store/settingsStore";
import { SearchDialog } from "./SearchDialog";

const mocks = vi.hoisted(() => ({ load: vi.fn(), search: vi.fn(), cancel: vi.fn() }));
vi.mock("@/store/capsStore", async original => ({ ...await original<typeof import("@/store/capsStore")>(), loadCaps: mocks.load }));
vi.mock("@/ipc/domains/search", () => ({ search: mocks.search, cancelSearch: mocks.cancel }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => {
  useDemo();
  mocks.load.mockReset().mockResolvedValue([]);
  mocks.search.mockReset().mockResolvedValue({ status: "complete", hits: 0, files: 0, dirs: 0, unreadable: 0 });
  mocks.cancel.mockReset().mockResolvedValue(undefined);
  useCaps.setState({ known: { local: ["search"] } });
  usePanes.setState(s => ({ left: { ...s.left, tabs: [{ ...s.left.tabs[0], path: "/src" }] } }));
  useSearchDialog.getState().open("left");
});

it.each(["stop", "close"])("does not dispatch after %s during preflight", async action => {
  const preflight = deferred<never[]>();
  mocks.load.mockReturnValue(preflight.promise);
  render(<SearchDialog />);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: translate("search.run") }));
  await waitFor(() => expect(mocks.load).toHaveBeenCalledOnce());
  if (action === "stop") await user.click(screen.getByRole("button", { name: translate("search.stop") }));
  else await user.keyboard("{Escape}");
  await act(async () => preflight.resolve([]));
  expect(mocks.search).not.toHaveBeenCalled();
  expect(mocks.cancel).not.toHaveBeenCalled();
});

it("ignores an old preflight after a replacement search", async () => {
  const old = deferred<never[]>();
  mocks.load.mockReturnValueOnce(old.promise);
  render(<SearchDialog />);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: translate("search.run") }));
  await user.click(screen.getByRole("button", { name: translate("search.stop") }));
  await user.click(screen.getByRole("button", { name: translate("search.run") }));
  await waitFor(() => expect(mocks.search).toHaveBeenCalledOnce());
  await act(async () => old.resolve([]));
  expect(mocks.search).toHaveBeenCalledOnce();
});

it("does not replace a newer result with an old native summary", async () => {
  const old = deferred<{ status: string; hits: number; files: number; dirs: number; unreadable: number }>();
  mocks.search.mockReturnValueOnce(old.promise);
  render(<SearchDialog />);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: translate("search.run") }));
  await waitFor(() => expect(mocks.search).toHaveBeenCalledOnce());
  await user.click(screen.getByRole("button", { name: translate("search.stop") }));
  expect(mocks.cancel).toHaveBeenCalledOnce();
  await user.click(screen.getByRole("button", { name: translate("search.run") }));
  await waitFor(() => expect(mocks.search).toHaveBeenCalledTimes(2));
  await act(async () => old.resolve({ status: "limited", hits: 99, files: 99, dirs: 0, unreadable: 0 }));
  expect(screen.queryByText(translate("search.limited"))).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: translate("search.run") })).toBeEnabled();
});

it.each(["de", "en"] as const)("shows a structured search rejection instead of empty results in %s", async language => {
  useSettings.setState({ language });
  mocks.search.mockRejectedValue({ version: 1, code: "fs.io", params: {}, op: "search", provider: "local",
    retryable: false, action: null, detail: "private diagnostic", id: "search-failure" });
  render(<SearchDialog />);
  await userEvent.setup().click(screen.getByRole("button", { name: translate("search.run") }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent(translate("error.fs.io"));
  expect(alert).not.toHaveTextContent("private diagnostic");
  expect(screen.queryByText(translate("search.empty"))).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: translate("search.run") })).toBeEnabled();
});
