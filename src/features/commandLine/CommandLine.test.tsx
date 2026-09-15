import { beforeEach, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useDemo, useTauri } from "@/test/tauri";

beforeEach(() => vi.resetModules());

async function setup(tauri = true, local = true) {
  const result = { id: 1, done: false, cancelled: false, exit_code: null, stdout: "hello", stderr: "", truncated: false, error: null };
  const harness = tauri ? useTauri({ commands: {
    command_start: () => 1,
    command_status: () => ({ ...result }),
    command_cancel: () => { Object.assign(result, { done: true, cancelled: true }); },
  } }) : (useDemo(), null);
  const { useCaps } = await import("@/store/capsStore");
  useCaps.setState({ known: { local: local ? ["localTerminal"] : [] } });
  const { default: CommandLine } = await import("./CommandLine");
  const view = render(<CommandLine path="/tmp/a folder" />);
  return { harness, view, input: screen.getByRole("textbox"), result };
}

it("submits the exact command and folder once, displays output and cancels", async () => {
  const { input, harness } = await setup();
  fireEvent.change(input, { target: { value: "printf 'a b'" } });
  fireEvent.keyDown(input, { key: "Enter" });
  fireEvent.keyDown(input, { key: "Enter" });
  await screen.findByText("hello");
  expect(harness!.calls.filter(c => c.cmd === "command_start")).toEqual([{ cmd: "command_start", args: { path: "/tmp/a folder", command: "printf 'a b'" } }]);
  expect(input).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Abbrechen" }));
  await screen.findByText("Abgebrochen");
  expect(input).toBeEnabled();
});

it("does not execute blank input, composition, modified Enter, or menu/dialog keys", async () => {
  const { input, harness } = await setup();
  fireEvent.keyDown(input, { key: "Enter" });
  fireEvent.change(input, { target: { value: "echo hi" } });
  fireEvent.keyDown(input, { key: "Enter", repeat: true });
  fireEvent.keyDown(input, { key: "Enter", isComposing: true });
  fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });
  for (const role of ["menu", "dialog", "alertdialog"]) {
    const overlay = document.createElement("div");
    overlay.setAttribute("role", role); document.body.append(overlay);
    fireEvent.keyDown(input, { key: "Enter" }); overlay.remove();
  }
  expect(harness!.calls).toEqual([]);
  fireEvent.keyDown(input, { key: "Escape" });
  expect(input).toHaveValue("");
});

it("keeps execution unavailable in the demo and unsupported locations", async () => {
  const { input } = await setup(false);
  expect(input).toBeDisabled();
});

it("keeps execution unavailable when local capability is missing", async () => {
  const { input } = await setup(true, false);
  expect(input).toBeDisabled();
});

it("shows exit codes and restores focus when output is closed", async () => {
  const { input, result } = await setup();
  Object.assign(result, { done: true, exit_code: 7, stderr: "problem" });
  fireEvent.change(input, { target: { value: "exit 7" } });
  fireEvent.keyDown(input, { key: "Enter" });
  await screen.findByText("Exit-Code: 7");
  expect(screen.getByText("problem")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Ausgabe schließen" }));
  await waitFor(() => expect(input).toHaveFocus());
  expect(screen.queryByText("problem")).not.toBeInTheDocument();
});
