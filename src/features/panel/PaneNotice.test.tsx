import { act, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useDemo } from "@/test/tauri";
import { useNotices } from "@/store/noticeStore";
import { useSettings } from "@/store/settingsStore";
import { PaneNotice } from "./PaneNotice";

beforeEach(() => { useDemo(); vi.useFakeTimers(); });

it.each(["de", "en"] as const)("announces results and preserves errors until dismissed in %s", language => {
  useSettings.setState({ language });
  render(<PaneNotice side="left" />);
  const live = screen.getByRole("status");
  expect(live).toBeEmptyDOMElement();
  act(() => useNotices.getState().show("left", "sel.restored", { n: 3 }));
  expect(live).toHaveTextContent(language === "de" ? "3 Eintrag/Einträge wiederhergestellt" : "3 entries restored");
  const first = live.firstElementChild;
  act(() => useNotices.getState().show("left", "sel.restored", { n: 3 }));
  expect(live.firstElementChild).not.toBe(first);
  act(() => vi.advanceTimersByTime(8_000));
  expect(live).toBeEmptyDOMElement();
  act(() => useNotices.getState().show("left", "sel.copyError", { err: "test error" }, "error"));
  act(() => vi.advanceTimersByTime(60_000));
  expect(live).toHaveTextContent("test error");
  expect(useNotices.getState().left?.tone).toBe("error");
  act(() => useNotices.getState().clear("left"));
  expect(live).toBeEmptyDOMElement();
});
