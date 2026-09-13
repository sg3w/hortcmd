import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import axe from "axe-core";
import { useDemo } from "@/test/tauri";
import { translate, useSettings } from "@/store/settingsStore";
import { openSettings } from "@/store/uiStore";
import { SettingsDialog } from "./SettingsDialog";

it.each(["de", "en"] as const)("changes settings and navigates categories with the keyboard in %s", async language => {
  useDemo();
  useSettings.setState({ language, theme: "dark" });
  render(<><button onClick={() => openSettings("general")}>{translate("settings.title")}</button><SettingsDialog /></>);
  const user = userEvent.setup();
  await user.tab();
  const trigger = screen.getByRole("button", { name: translate("settings.title") });
  await user.keyboard("{Enter}");
  const general = screen.getByRole("tab", { name: translate("settings.cat.general") });
  for (let i = 0; i < 20 && document.activeElement !== general; i++) await user.tab();
  expect(general).toHaveFocus();
  expect(general).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{End}");
  const favorites = screen.getByRole("tab", { name: translate("settings.cat.favorites") });
  expect(favorites).toHaveFocus();
  expect(favorites).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("tabpanel")).toHaveAccessibleName(translate("settings.cat.favorites"));
  await user.keyboard("{Home}{ArrowDown}{ArrowUp}");
  expect(general).toHaveFocus();
  await user.tab();
  const separator = screen.getByRole("separator", { name: translate("settings.categoryWidth") });
  expect(separator).toHaveFocus();
  await user.keyboard("{End}");
  expect(separator).toHaveAttribute("aria-valuenow", "50");
  await user.keyboard("{Home}{ArrowRight}");
  expect(separator).toHaveAttribute("aria-valuenow", "30");
  const light = within(screen.getByRole("group", { name: translate("settings.theme") }))
    .getByRole("button", { name: translate("settings.theme.light") });
  for (let i = 0; i < 20 && document.activeElement !== light; i++) await user.tab();
  expect(light).toHaveFocus();
  await user.keyboard(" ");
  expect(useSettings.getState().theme).toBe("light");
  expect(light).toHaveAttribute("aria-pressed", "true");
  const report = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(report.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
  await user.keyboard("{Escape}");
  await waitFor(() => expect(trigger).toHaveFocus());
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it.each(["de", "en"] as const)("has named controls on every settings page in %s", async language => {
  useDemo();
  useSettings.setState({ language });
  openSettings("general");
  render(<SettingsDialog />);
  const user = userEvent.setup();
  act(() => screen.getByRole("tab", { name: translate("settings.cat.general") }).focus());
  for (let page = 0; page < 8; page++) {
    const panel = screen.getByRole("tabpanel");
    const report = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
    expect(report.violations.filter(v => v.impact === "serious" || v.impact === "critical"), panel.getAttribute("aria-labelledby") ?? undefined).toEqual([]);
    await user.keyboard("{ArrowDown}");
  }
});
