import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import axe from "axe-core";
import { useTauri } from "@/test/tauri";
import { useSettings } from "@/store/settingsStore";
import { useTerminals } from "./terminalStore";
import { SshTerminalWindow } from "./SshTerminal";

vi.mock("@/ipc/invoke", async importOriginal => ({
  ...await importOriginal<typeof import("@/ipc/invoke")>(), hasTauri: () => true,
}));
const terminals = vi.hoisted(() => [] as { options: Record<string, unknown>; textarea: HTMLTextAreaElement }[]);
vi.mock("@xterm/xterm", () => ({ Terminal: class {
  static strings = { promptLabel: "", tooMuchOutput: "" };
  textarea = document.createElement("textarea");
  cols = 80; rows = 24;
  constructor(public options: Record<string, unknown>) { terminals.push(this); }
  open(host: HTMLElement) { this.textarea.setAttribute("aria-label", this.constructorStrings()); host.append(this.textarea); }
  constructorStrings() { return (this.constructor as { strings?: { promptLabel: string } }).strings?.promptLabel ?? ""; }
  focus() { this.textarea.focus(); }
  loadAddon() {} write() {} dispose() { this.textarea.remove(); }
  onData(callback: (data: string) => void) {
    const key = (event: KeyboardEvent) => {
      // xterm handles keys even when Radix has prevented its Escape dismissal.
      if (!["Control", "Shift"].includes(event.key)) callback(event.key);
    };
    this.textarea.addEventListener("keydown", key, true);
    return { dispose: () => this.textarea.removeEventListener("keydown", key, true) };
  }
} }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() {} } }));

beforeEach(() => {
  terminals.length = 0;
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  vi.stubGlobal("matchMedia", () => ({ matches: true, addEventListener() {}, removeEventListener() {} }));
});
afterEach(() => vi.unstubAllGlobals());

it.each(["de", "en"] as const)("provides terminal semantics and a keyboard exit in %s", async language => {
  useSettings.setState({ language });
  const harness = useTauri({ commands: {
    ssh_open_shell: () => "shell", ssh_send: () => {}, ssh_resize: () => {}, ssh_close: () => {},
  } });
  useTerminals.setState({ session: { uid: "terminal", address: "sftp://example.invalid", title: "test shell", sshId: null, status: "opening", reason: null } });
  const user = userEvent.setup();
  render(<SshTerminalWindow />);
  const input = screen.getByRole("textbox", { name: language === "de" ? "SSH-Terminaleingabe" : "SSH terminal input" });
  await waitFor(() => expect(input).toHaveFocus());
  expect(terminals[0].options.screenReaderMode).toBe(true);
  expect(terminals[0].options.cursorBlink).toBe(false);
  await user.keyboard("{Escape}");
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  await waitFor(() => expect(harness.calls.some(call => call.cmd === "ssh_send" && call.args.data === "Escape")).toBe(true));
  const sentBeforeExit = harness.calls.filter(call => call.cmd === "ssh_send").length;
  await user.keyboard("{Control>}{Shift>}{F6}{/Shift}{/Control}");
  expect(screen.getByRole("button", { name: language === "de" ? "Schließen" : "Close" })).toHaveFocus();
  expect(harness.calls.filter(call => call.cmd === "ssh_send")).toHaveLength(sentBeforeExit);
  const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(result.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
  act(() => useSettings.setState({ language: language === "de" ? "en" : "de" }));
  expect(input).toHaveAccessibleName(language === "de" ? "SSH terminal input" : "SSH-Terminaleingabe");
  expect(harness.calls.filter(call => call.cmd === "ssh_open_shell")).toHaveLength(1);
  await user.keyboard("{Enter}");
  expect(useTerminals.getState().session).toBeNull();
  await waitFor(() => expect(harness.calls.some(call => call.cmd === "ssh_close")).toBe(true));
});
