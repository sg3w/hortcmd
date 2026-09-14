import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import axe from "axe-core";
import { translate, useSettings } from "@/store/settingsStore";
import { useDemo } from "@/test/tauri";
import { useOps } from "@/store/opsStore";
import { promptArchivePassword, archivePassword } from "@/features/commander/archivePw";
import { promptSelect } from "@/features/commander/selection";
import { usePanes } from "@/store/panesStore";
import { OperationDialogs } from "./OperationDialogs";

beforeEach(() => useDemo());
it("preserves a follow-up prompt opened by confirmation", async () => {
  const user = userEvent.setup();
  useOps.getState().requestConfirm({
    title: "Confirm", message: "Continue", onConfirm: () => useOps.getState().requestPrompt({
      title: "Follow-up", label: "Name", initial: "retry", onSubmit: vi.fn(),
    }),
  });
  render(<OperationDialogs />);
  await user.click(screen.getByRole("button", { name: "OK" }));
  expect(screen.getByRole("dialog", { name: "Follow-up" })).toBeVisible();
  expect(useOps.getState().prompt?.initial).toBe("retry");
});


it.each(["de", "en"] as const)("names operation inputs and submits them by keyboard in %s", async language => {
  useSettings.setState({ language });
  const submitted = vi.fn();
  const user = userEvent.setup();
  useOps.getState().requestPrompt({ title: translate("op.rename.title"), label: translate("op.rename.label"),
    initial: "old.txt", onSubmit: submitted });
  render(<OperationDialogs />);
  const input = screen.getByRole("textbox", { name: translate("op.rename.label") });
  await waitFor(() => expect(input).toHaveFocus());
  await user.clear(input);
  await user.type(input, "renamed.txt");
  const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(result.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
  await user.keyboard("{Enter}");
  expect(submitted).toHaveBeenCalledWith("renamed.txt", false);
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it.each(["de", "en"] as const)("describes confirmation messages and exposes indeterminate progress in %s", async language => {
  useSettings.setState({ language });
  useOps.getState().requestConfirm({ title: translate("op.delete.title"), message: translate("op.delete.confirm").replace("{n}", "2"), onConfirm: vi.fn() });
  render(<OperationDialogs />);
  expect(screen.getByRole("dialog")).toHaveAccessibleDescription(translate("op.delete.confirm").replace("{n}", "2"));
  const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(result.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
  act(() => { useOps.getState().closeDialogs(); useOps.getState().setBusy(true); });
  expect(screen.getByRole("progressbar", { name: translate("op.progress") })).not.toHaveAttribute("aria-valuenow");
  expect(screen.getByRole("status")).toHaveTextContent(translate("op.progress"));
});

it("returns focus to the invoking control after a prompt and follow-up confirmation", async () => {
  const user = userEvent.setup();
  render(<><button onClick={() => useOps.getState().requestPrompt({
    title: "Rename", label: "Name", initial: "old.txt", onSubmit: () => useOps.getState().requestConfirm({
      title: "Result", message: "Finished", onConfirm: vi.fn(),
    }),
  })}>Start</button><OperationDialogs /></>);
  const trigger = screen.getByRole("button", { name: "Start" });
  await user.click(trigger);
  await waitFor(() => expect(screen.getByRole("textbox", { name: "Name" })).toHaveFocus());
  await user.keyboard("{Enter}");
  await screen.findByRole("dialog", { name: "Result" });
  await user.click(screen.getByRole("button", { name: "OK" }));
  await waitFor(() => expect(trigger).toHaveFocus());
});

for (const method of ["Enter", "button"] as const) {
  it.each(["", "   ", " new.txt "])(`validates name %j via ${method} without changing the submitted value`, async value => {
    const submitted = vi.fn();
    const user = userEvent.setup();
    useOps.getState().requestPrompt({ title: "Name", label: "Value", initial: "", confirmLabel: "Submit", onSubmit: submitted });
    render(<OperationDialogs />);
    const input = screen.getByLabelText("Value");
    await waitFor(() => expect(input).toHaveFocus());
    fireEvent.change(input, { target: { value } });
    const button = screen.getByRole("button", { name: "Submit" });
    const valid = !!value.trim();
    if (valid) expect(button).toBeEnabled();
    else expect(button).toBeDisabled();
    if (method === "Enter") await user.keyboard("{Enter}");
    else await user.click(button);
    if (valid) {
      expect(submitted).toHaveBeenCalledExactlyOnceWith(value, false);
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    } else {
      expect(submitted).not.toHaveBeenCalled();
      expect(screen.getByRole("dialog")).toBeVisible();
      expect(input).toHaveFocus();
    }
  });

  it.each(["", "   ", "  päss\t "])(`applies the archive password policy to %j via ${method}`, async value => {
    const submitted = vi.fn();
    const user = userEvent.setup();
    const archive = `/test/${method}-${value.length}.7z`;
    promptArchivePassword(archive, false, submitted);
    render(<OperationDialogs />);
    const input = screen.getByLabelText(translate("archive.password.label"));
    await waitFor(() => expect(input).toHaveFocus());
    expect(input).toHaveAttribute("type", "password");
    fireEvent.change(input, { target: { value } });
    const button = screen.getByRole("button", { name: translate("op.confirm") });
    if (value.length) expect(button).toBeEnabled();
    else expect(button).toBeDisabled();
    if (method === "Enter") await user.keyboard("{Enter}");
    else await user.click(button);
    if (value.length) {
      expect(submitted).toHaveBeenCalledExactlyOnceWith(value);
      expect(archivePassword(archive)).toBe(value);
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    } else {
      expect(submitted).not.toHaveBeenCalled();
      expect(screen.getByRole("dialog")).toBeVisible();
      expect(input).toHaveFocus();
    }
  });

  it.each([true, false])(`explicitly accepts an empty selection pattern via ${method} (add=%s)`, async add => {
    const select = vi.spyOn(usePanes.getState(), "selectPattern").mockImplementation(() => {});
    const user = userEvent.setup();
    promptSelect("left", add);
    render(<OperationDialogs />);
    const input = screen.getByLabelText(translate("sel.pattern"));
    await waitFor(() => expect(input).toHaveFocus());
    fireEvent.change(input, { target: { value: add ? "" : "   " } });
    const button = screen.getByRole("button", { name: translate("op.confirm") });
    expect(button).toBeEnabled();
    if (method === "Enter") await user.keyboard("{Enter}");
    else await user.click(button);
    expect(select).toHaveBeenCalledExactlyOnceWith("left", "*", add);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
}
