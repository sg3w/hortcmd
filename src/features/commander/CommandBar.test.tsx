import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { useDemo } from "@/test/tauri";
import { useUi } from "@/store/uiStore";
import { translate } from "@/store/settingsStore";
import { CommandBar } from "./CommandBar";

it.each(["general", "view"] as const)("opens settings from the toolbar without replacing category %s with the click event", async category => {
  useDemo();
  useUi.setState({ settingsCategory: category });
  render(<CommandBar />);
  await userEvent.setup().click(screen.getByRole("button", { name: translate("tb.settings") }));
  expect(useUi.getState().settingsOpen).toBe(true);
  expect(useUi.getState().settingsCategory).toBe(category);
});
