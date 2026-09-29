import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useDemo } from "@/test/tauri";
import { useSettings } from "@/store/settingsStore";
import { useNotices } from "@/store/noticeStore";
import { useDialogSizeStore } from "@/store/dialogSizeStore";
import {
  PERSISTED_KEYS,
  quarantinedKeys,
  rehydrate,
  resetRecovered,
} from "@/store/persisted/gate";
import App from "./App";
import axe from "axe-core";

afterEach(() => {
  vi.unstubAllGlobals();
  Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
  delete document.documentElement.dataset.theme;
});

it.each(["de", "en"] as const)("has no serious accessibility violations in the loaded shell in %s", async language => {
  useDemo();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  vi.stubGlobal("matchMedia", () => ({ matches: false, addEventListener() {}, removeEventListener() {} }));
  useSettings.setState({ language });
  render(<App />);
  await screen.findByRole("grid", { name: language === "de" ? "Linke Dateiliste" : "Left file list" });
  const report = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(report.violations.filter(v => v.impact === "serious" || v.impact === "critical")
    .map(v => ({ id: v.id, nodes: v.nodes.map(node => node.html) }))).toEqual([]);
});

it("follows system appearance changes until an explicit theme is chosen", async () => {
  useDemo();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  const listeners = new Set<() => void>();
  const media = { matches: false, media: "(prefers-color-scheme: dark)",
    addEventListener: (_event: string, callback: () => void) => listeners.add(callback),
    removeEventListener: (_event: string, callback: () => void) => listeners.delete(callback),
  };
  vi.stubGlobal("matchMedia", (query: string) => query === media.media ? media : {
    matches: false, addEventListener() {}, removeEventListener() {},
  });
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  useSettings.setState({ theme: "system" });
  const view = render(<App />);
  await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "light"));
  expect(listeners.size).toBe(1);
  act(() => { media.matches = true; listeners.forEach(callback => callback()); });
  await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "dark"));
  act(() => useSettings.getState().setTheme("light"));
  await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "light"));
  expect(listeners.size).toBe(0);
  act(() => { media.matches = true; listeners.forEach(callback => callback()); });
  expect(document.documentElement).toHaveAttribute("data-theme", "light");
  act(() => useSettings.getState().setTheme("system"));
  await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "dark"));
  expect(listeners.size).toBe(1);
  view.unmount();
  expect(listeners.size).toBe(0);
});

// ------------------------------------------------------------
// The startup notice about recovered persisted state (TICKET-389).
//
// Rendering the real shell is the point: the write-back only works if
// the store that registered it is the one that hydrated, and that is
// exactly what a unit test of the gate cannot show.
// ------------------------------------------------------------

/** Prepares the shell and leaves `dialogSizes` looking repaired. */
function shellWithARepairedDomain() {
  useDemo();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: vi.fn(),
  });
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  vi.stubGlobal("matchMedia", () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  }));
  vi.spyOn(console, "warn").mockImplementation(() => {});
  resetRecovered();
  useNotices.setState({ left: null, right: null });
  useSettings.setState({ language: "en" });
  localStorage.clear();
  useDialogSizeStore.setState({ sizes: { test: { w: 612, h: 400 } } });
  // Something has to be there for the quarantine copy to be made of.
  localStorage.setItem(PERSISTED_KEYS.dialogSizes, '{"state":{"sizes":{}}}');
  rehydrate("dialogSizes", { sizes: {} }, (value, r) => {
    r.note("sizes");
    return value;
  });
}

it("reports a corrected value as a correction and writes it back", async () => {
  shellWithARepairedDomain();
  const view = render(<App />);

  await waitFor(() =>
    expect(useNotices.getState().left?.key).toBe("persist.repaired"),
  );
  // Not an error: nothing the user configured was lost.
  expect(useNotices.getState().left?.tone).toBe("info");
  // And the repaired value is back in storage, so the next start has
  // nothing left to repair and nothing left to report.
  expect(localStorage.getItem(PERSISTED_KEYS.dialogSizes)).toContain('"w":612');
  view.unmount();
});

it("says nothing about recovered data once the notice is switched off", async () => {
  shellWithARepairedDomain();
  useSettings.setState({ reportPersistRecovery: false });
  const view = render(<App />);

  await screen.findByRole("grid", { name: "Left file list" });
  expect(useNotices.getState().left).toBeNull();
  // Switching the notice off is about the notice: the value is still
  // written back, and the quarantined copy is still there to be read.
  expect(localStorage.getItem(PERSISTED_KEYS.dialogSizes)).toContain('"w":612');
  expect(quarantinedKeys().length).toBe(1);
  view.unmount();
  useSettings.setState({ reportPersistRecovery: true });
});
