import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useDemo } from "@/test/tauri";
import { useSettings } from "@/store/settingsStore";
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
