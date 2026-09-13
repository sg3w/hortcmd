import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { file } from "@/test/fixtures";
import { usePanes } from "@/store/panesStore";
import { useCaps } from "@/store/capsStore";
import { translate, useSettings } from "@/store/settingsStore";
import { describeErrorFull } from "@/store/errorText";
import type { AppError } from "@/ipc/bindings";
import { OperationDialogs } from "@/components/ui/OperationDialogs";
import { useKeyboard } from "./useKeyboard";

// Keep the real domain clients and command arguments; only select the runtime.
vi.mock("@/ipc/invoke", async importOriginal => ({
  ...await importOriginal<typeof import("@/ipc/invoke")>(), hasTauri: true,
}));

function Host() {
  useKeyboard();
  return <><button>{translate("op.rename.title")}</button><OperationDialogs /></>;
}

function setup(language: "de" | "en") {
  const runtime = useTauri({ commands: {
    copy_entries: () => {}, move_entries: () => {}, rename_entry: () => {},
    trash_entries: () => ({ ok: 1, errors: [] }),
    list_dir: ({ path }) => ({ path, entries: [file("renamed.txt")] }),
    request_git_status: () => {},
  } });
  useSettings.setState({ language, useTrash: true });
  useCaps.setState({ known: { local: ["browse", "copyFrom", "copyTo", "move", "deletePermanent", "trash"] } });
  for (const side of ["left", "right"] as const) {
    const entries = [file("original.txt"), file("marked.txt")];
    usePanes.setState(s => ({ active: "left", [side]: { ...s[side], activeTab: 0,
      tabs: [{ ...s[side].tabs[0], path: side === "left" ? "/src" : "/dst",
        archive: null, raw: entries, entries, cursor: 0, selected: new Set(["marked.txt"]) }],
    } }));
  }
  render(<Host />);
  // Start with workspace focus; body Tab intentionally switches the active pane.
  screen.getByRole("button", { name: translate("op.rename.title") }).focus();
  return { runtime, user: userEvent.setup() };
}

it.each(["de", "en"] as const)("renames by F2 and Enter and restores keyboard focus in %s", async language => {
  const { runtime, user } = setup(language);
  const trigger = screen.getByRole("button", { name: translate("op.rename.title") });
  expect(trigger).toHaveFocus();
  await user.keyboard("{F2}");
  const input = screen.getByRole("textbox", { name: translate("op.rename.label") });
  await waitFor(() => expect(input).toHaveFocus());
  await waitFor(() => {
    expect(input).toHaveValue("original.txt");
    expect((input as HTMLInputElement).selectionStart).toBe(0);
    expect((input as HTMLInputElement).selectionEnd).toBe("original".length);
  });
  // Replace the preselected base name; preserve the extension.
  await user.keyboard("renamed{Enter}");
  await waitFor(() => expect(runtime.calls).toContainEqual({ cmd: "rename_entry", args: {
    from: "/src/original.txt", to: "/src/renamed.txt",
  } }));
  await waitFor(() => expect(trigger).toHaveFocus());
  expect(runtime.calls.filter(c => c.cmd === "rename_entry")).toHaveLength(1);
  expect(usePanes.getState().left.tabs[0].filter).toBe("");
});

for (const [key, command] of [["F5", "copy_entries"], ["F6", "move_entries"]] as const) {
  it.each(["de", "en"] as const)(`${key} dispatches the selected file to the opposite pane in %s`, async language => {
    const { runtime, user } = setup(language);
      await user.keyboard(`{${key}}`);
    await waitFor(() => expect(runtime.calls.filter(c => c.cmd === command)).toHaveLength(1));
    expect(runtime.calls.find(c => c.cmd === command)?.args).toMatchObject({
      sources: ["/src/marked.txt"], destDir: "/dst",
    });
    expect(runtime.calls.some(c => c.cmd === (key === "F5" ? "move_entries" : "copy_entries"))).toBe(false);
  });
}

it.each(["de", "en"] as const)("cancels and confirms trash by keyboard in %s", async language => {
  const { runtime, user } = setup(language);
  const trigger = document.activeElement;
  await user.keyboard("{F8}");
  expect(screen.getByRole("dialog", { name: translate("op.trash.title") })).toBeVisible();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(trigger).toHaveFocus());
  expect(runtime.calls.some(c => c.cmd === "trash_entries")).toBe(false);
  await user.keyboard("{F8}");
  await waitFor(() => expect(screen.getByRole("button", { name: translate("op.trash.button") })).toHaveFocus());
  await user.keyboard("{Enter}");
  await waitFor(() => expect(runtime.calls).toContainEqual({ cmd: "trash_entries", args: { paths: ["/src/marked.txt"] } }));
  await waitFor(() => expect(trigger).toHaveFocus());
  expect(runtime.calls.filter(c => c.cmd === "trash_entries")).toHaveLength(1);
  expect(runtime.calls.some(c => c.cmd === "delete_entries")).toBe(false);
});

it.each(["de", "en"] as const)("keeps an asynchronous rename error keyboard-accessible in %s", async language => {
  const { runtime, user } = setup(language);
  const error: AppError = { version: 1, code: "fs.io", params: {}, op: "rename",
    provider: "local", retryable: false, action: null, detail: "private diagnostic",
    id: "rename-keyboard-test" };
  runtime.setCommand("rename_entry", async () => { throw error; });
  const trigger = document.activeElement;
  await user.keyboard("{F2}");
  const input = screen.getByRole("textbox", { name: translate("op.rename.label") });
  await waitFor(() => expect((input as HTMLInputElement).selectionEnd).toBe("original".length));
  await user.keyboard("changed{Enter}");
  const dialog = await screen.findByRole("dialog", { name: translate("op.errors") });
  expect(dialog).toHaveAccessibleDescription(describeErrorFull(error));
  expect(dialog).not.toHaveTextContent("private diagnostic");
  await waitFor(() => expect(screen.getByRole("button", { name: translate("op.confirm") })).toHaveFocus());
  await user.keyboard("{Enter}");
  await waitFor(() => expect(trigger).toHaveFocus());
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(runtime.calls.filter(c => c.cmd === "rename_entry")).toHaveLength(1);
});
