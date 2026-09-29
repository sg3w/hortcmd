import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import axe from "axe-core";
import { dir, file } from "@/test/fixtures";
import { useDemo } from "@/test/tauri";
import { useOps } from "@/store/opsStore";
import { translate, useSettings } from "@/store/settingsStore";
import { PackDialog } from "./PackDialog";

const ENTRIES = [file("notes.txt"), dir("Fotos")];
const STAMP = /\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}/;

beforeEach(() => {
  useDemo();
  useSettings.setState({ language: "en" });
});

/** Opens the dialog over `ENTRIES` and returns the name field. */
async function open(onSubmit = vi.fn()) {
  useOps.getState().requestPack({ entries: ENTRIES, onSubmit });
  render(<PackDialog />);
  const input: HTMLInputElement = screen.getByRole("textbox", { name: translate("archive.label") });
  await waitFor(() => expect(input).toHaveFocus());
  return { input, onSubmit };
}

it("opens with the proposal and preselects everything but the extension", async () => {
  const { input } = await open();
  expect(input.value).toMatch(new RegExp(`^Fotos_${STAMP.source}\\.zip$`));
  expect(input.value.slice(input.selectionStart ?? 0, input.selectionEnd ?? 0)).toBe(
    input.value.slice(0, -4),
  );
  const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(result.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
});

it("rebuilds the name when date, time or base change", async () => {
  const user = userEvent.setup();
  const { input } = await open();

  await user.click(screen.getByRole("checkbox", { name: translate("archive.addDate") }));
  expect(input.value).toMatch(/^Fotos_\d{2}-\d{2}-\d{2}\.zip$/);

  await user.click(screen.getByRole("checkbox", { name: translate("archive.addTime") }));
  expect(input.value).toBe("Fotos.zip");

  await user.selectOptions(screen.getByRole("combobox", { name: translate("archive.base") }), "generic");
  expect(input.value).toBe("archiv.zip");
});

it("disables the options once the name is typed and hands them back on request", async () => {
  const user = userEvent.setup();
  const { input } = await open();
  const takeover = screen.getByRole("checkbox", { name: translate("archive.auto") });
  const addDate = screen.getByRole("checkbox", { name: translate("archive.addDate") });
  expect(takeover).toBeChecked();

  await user.clear(input);
  await user.type(input, "Urlaub");
  expect(takeover).not.toBeChecked();
  expect(addDate).toBeDisabled();
  expect(screen.getByRole("combobox", { name: translate("archive.base") })).toBeDisabled();
  expect(screen.getByText(translate("archive.auto.hint"))).toBeVisible();

  await user.click(takeover);
  expect(addDate).toBeEnabled();
  expect(input.value).toMatch(new RegExp(`^Fotos_${STAMP.source}\\.zip$`));
});

it("submits the name in the field", async () => {
  const user = userEvent.setup();
  const { input, onSubmit } = await open();
  await user.clear(input);
  await user.type(input, "Urlaub.zip");
  await user.keyboard("{Enter}");
  expect(onSubmit).toHaveBeenCalledWith("Urlaub.zip");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("refuses a blank name and closes on Escape", async () => {
  const user = userEvent.setup();
  const { input, onSubmit } = await open();
  await user.clear(input);
  expect(screen.getByRole("button", { name: translate("op.create") })).toBeDisabled();
  await user.keyboard("{Escape}");
  expect(onSubmit).not.toHaveBeenCalled();
  expect(useOps.getState().pack).toBeNull();
});
