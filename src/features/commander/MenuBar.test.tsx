import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import axe from "axe-core";
import { useDemo } from "@/test/tauri";
import { useSettings } from "@/store/settingsStore";
import { panelOf, usePanes } from "@/store/panesStore";
import { MenuBar } from "./MenuBar";
import { useKeyboard } from "./useKeyboard";

beforeEach(() => useDemo());
function Host() { useKeyboard(); return <MenuBar />; }

it.each(["de", "en"] as const)("supports menu keys, restores focus and isolates pane actions in %s", async language => {
  useSettings.setState({ language });
  render(<Host />);
  const user = userEvent.setup();
  const files = screen.getByRole("menuitem", { name: language === "de" ? "Dateien" : "Files" });
  act(() => files.focus());
  await user.keyboard("{ArrowRight}");
  const mark = screen.getByRole("menuitem", { name: language === "de" ? "Markieren" : "Select" });
  expect(mark).toHaveFocus();
  await user.keyboard("{Enter}{ArrowDown}x");
  expect(screen.getByRole("menu")).toBeVisible();
  expect(panelOf(usePanes.getState(), "left").filter).toBe("");
  const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  expect(result.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  expect(mark).toHaveFocus();
});
