import { act, render, screen } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import type { ItemError } from "@/ipc/bindings";
import { useDemo } from "@/test/tauri";
import { useTransfers } from "@/store/transfersStore";
import { useSettings } from "@/store/settingsStore";
import { TransferWindow } from "./TransferView";

beforeEach(() => useDemo());

it("renders structured transfer errors and updates them when the language changes", () => {
  const error: ItemError = {
    item: "report.txt",
    error: {
      version: 1, code: "fs.notFound", params: {}, op: "copy", provider: "local",
      retryable: false, action: null, detail: "private diagnostic detail", id: "e-transfer-test",
    },
  };
  useTransfers.getState().start("copy", "failed-transfer");
  useTransfers.setState(s => ({ transfers: s.transfers.map(t => ({ ...t, done: true, errors: [error] })) }));
  render(<TransferWindow />);
  expect(screen.getByText("report.txt: Nicht gefunden")).toBeVisible();
  expect(screen.queryByText(/\[object Object\]|private diagnostic detail/)).not.toBeInTheDocument();
  act(() => useSettings.setState({ language: "en" }));
  expect(screen.getByText("report.txt: Not found")).toBeVisible();
  expect(screen.queryByText(/Nicht gefunden/)).not.toBeInTheDocument();
});
