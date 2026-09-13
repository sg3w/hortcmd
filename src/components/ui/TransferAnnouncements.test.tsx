import { act, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import axe from "axe-core";
import { useDemo } from "@/test/tauri";
import { useTransfers, type Transfer } from "@/store/transfersStore";
import { useSettings } from "@/store/settingsStore";
import { TransferWindow } from "./TransferView";

beforeEach(() => useDemo());
function update(changes: Partial<Transfer>) {
  act(() => useTransfers.setState(s => ({ transfers: s.transfers.map(t => ({ ...t, ...changes })) })));
}

it.each(["de", "en"] as const)("throttles background progress but announces state changes immediately in %s", language => {
  vi.useFakeTimers();
  useSettings.setState({ language });
  render(<TransferWindow />);
  const live = screen.getByRole("status");
  expect(live).toBeEmptyDOMElement();
  act(() => useTransfers.getState().start("copy", "transfer"));
  const initial = live.textContent;
  update({ minimized: true, bytesTotal: 100, bytesDone: 12, fileName: "report.txt" });
  expect(live.textContent).toBe(initial);
  act(() => vi.advanceTimersByTime(9_999));
  expect(live.textContent).toBe(initial);
  update({ bytesDone: 35 });
  act(() => vi.advanceTimersByTime(1));
  expect(live).toHaveTextContent(language === "de" ? "35 Prozent" : "35 percent");
  update({ paused: true });
  expect(live).toHaveTextContent(language === "de" ? "Pausiert" : "Paused");
  act(() => vi.advanceTimersByTime(20_000));
  expect(live).toHaveTextContent(language === "de" ? "Pausiert" : "Paused");
  update({ paused: false, done: true });
  expect(live).toHaveTextContent(language === "de" ? "Abgeschlossen" : "Completed");
  act(() => vi.advanceTimersByTime(20_000));
  expect(live).toHaveTextContent(language === "de" ? "Abgeschlossen" : "Completed");
  act(() => useSettings.setState({ language: language === "de" ? "en" : "de" }));
  expect(live).toHaveTextContent(language === "de" ? "Completed" : "Abgeschlossen");
});

it("announces cancellation when a queued transfer is removed", () => {
  useTransfers.getState().start("move", "queued");
  useTransfers.getState().setQueued("queued", true);
  render(<TransferWindow />);
  expect(screen.getByRole("status")).toHaveTextContent("Wartet");
  act(() => useTransfers.getState().remove("queued"));
  expect(screen.getByRole("status")).toHaveTextContent("Abgebrochen");
});

it.each(["de", "en"] as const)("exposes named progress controls without serious axe violations in %s", async language => {
  useSettings.setState({ language });
  useTransfers.getState().start("copy", "visible");
  render(<main><TransferWindow /></main>);
  update({ bytesDone: 25, bytesTotal: 100 });
  const progress = screen.getByRole("progressbar", { name: language === "de" ? "Gesamt" : "Overall" });
  expect(progress).toHaveAttribute("aria-valuenow", "25");
  const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(result.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
});
