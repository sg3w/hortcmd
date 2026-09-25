import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import axe from "axe-core";
import type { AppError, ConnectionStatus, Profile } from "@/ipc/bindings";
import { useTauri } from "@/test/tauri";
import { useSettings, translate } from "@/store/settingsStore";
import { usePanes } from "@/store/panesStore";
import { useNet } from "./netStore";
import { ConnectionsDialog } from "./ConnectionsDialog";

vi.mock("@/ipc/invoke", async importOriginal => ({
  ...await importOriginal<typeof import("@/ipc/invoke")>(), hasTauri: () => true,
}));

function setup(language: "de" | "en", overrides: Partial<Profile> = {}) {
  let profile: Profile = { security: { plaintextAcknowledged: false, certificate: null }, id: "p1", name: "Example", protocol: "sftp", host: "example.test",
    port: 22, user: "demo", auth: "agent", key_path: null, start_path: "/", ...overrides };
  let connections: ConnectionStatus[] = [];
  const address = "sftp://demo@example.test:22/";
  const runtime = useTauri({ commands: {
    net_transport_policy: () => ({ allowPlaintext: false }),
    net_list_profiles: () => [profile], net_list_connections: () => connections,
    net_cred_status: () => ({ backend: "keychain", needs_unlock: false, unlocked: true }),
    net_save_profile: ({ profile: saved }) => { profile = saved as Profile; },
    net_cancel_connect: () => { connections = []; },
    net_connect: () => {
      connections = [{ id: "p1", key: "demo@example.test:22", name: profile.name,
        connected: true, phase: "connected", address, attempt: null, error: null }];
      return { kind: "connected", data: connections[0] };
    },
    list_dir: ({ path }) => ({ path, entries: [] }),
    request_git_status: () => {},
    location_capabilities: () => ({ scheme: "sftp", store_id: "demo@example.test:22", connected: true, caps: ["browse"] }),
  } });
  useSettings.setState({ language });
  render(<><button onClick={() => useNet.getState().openManager()}>{translate("net.title")}</button><ConnectionsDialog /></>);
  const trigger = screen.getByRole("button", { name: translate("net.title") });
  trigger.focus();
  const user = userEvent.setup();
  const tabTo = async (element: HTMLElement) => {
    for (let i = 0; i < 25 && document.activeElement !== element; i++) await user.tab();
    expect(element).toHaveFocus();
  };
  return { runtime, user, trigger, tabTo, address };
}

it.each(["de", "en"] as const)("edits a profile and connects using only the keyboard in %s", async language => {
  const { runtime, user, trigger, tabTo, address } = setup(language);
  await user.keyboard("{Enter}");
  const group = await screen.findByRole("group", { name: "Example" });
  await tabTo(within(group).getByRole("button", { name: translate("net.edit") }));
  await user.keyboard("{Enter}");
  const name = screen.getByRole("textbox", { name: translate("net.name") });
  await tabTo(name);
  await user.keyboard("{ArrowRight}{Backspace>7/}Renamed");
  const report = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(report.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
  await tabTo(screen.getByRole("button", { name: translate("net.save") }));
  await user.keyboard("{Enter}");
  const saved = await screen.findByRole("group", { name: "Renamed" });
  expect(runtime.calls.find(call => call.cmd === "net_save_profile")?.args).toMatchObject({ profile: { id: "p1", name: "Renamed" }, secret: null });
  await tabTo(within(saved).getByRole("button", { name: translate("net.connect") }));
  await user.keyboard("{Enter}");
  await waitFor(() => expect(usePanes.getState().left.tabs[0].path).toBe(address));
  await waitFor(() => expect(trigger).toHaveFocus());
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(runtime.calls.filter(call => call.cmd === "net_connect")).toEqual([{ cmd: "net_connect", args: { id: "p1" } }]);
});

it.each(["de", "en"] as const)("keeps busy actions named and cancels an attempt by keyboard in %s", async language => {
  const { runtime, user, tabTo, address } = setup(language);
  const pending: ConnectionStatus = { id: "p1", key: "demo@example.test:22", name: "Example",
    address, connected: false, phase: "connecting", attempt: "attempt-1", error: null };
  runtime.setCommand("net_connect", () => ({ kind: "pending", data: pending }));
  await user.keyboard("{Enter}");
  const group = await screen.findByRole("group", { name: "Example" });
  await tabTo(within(group).getByRole("button", { name: translate("net.connect") }));
  await user.keyboard("{Enter}");
  const cancel = await within(group).findByRole("button", { name: translate("net.cancelConnect") });
  expect(within(group).getByRole("button", { name: translate("net.edit") })).toBeDisabled();
  expect(within(group).getByRole("button", { name: translate("net.delete") })).toBeDisabled();
  expect(within(group).getByRole("button", { name: translate("hostkey.forget") })).toBeDisabled();
  expect(group).toHaveTextContent(translate("net.connecting"));
  await tabTo(cancel);
  await user.keyboard("{Enter}");
  await within(group).findByRole("button", { name: translate("net.connect") });
  expect(runtime.calls.filter(call => call.cmd === "net_cancel_connect")).toEqual([
    { cmd: "net_cancel_connect", args: { attempt: "attempt-1" } },
  ]);
});

it.each(["de", "en"] as const)("announces a failed keyboard connection and keeps the dialog reachable in %s", async language => {
  const { runtime, user, trigger, tabTo } = setup(language);
  const error: AppError = { version: 1, code: "net.auth", params: {}, op: "connect", provider: "sftp",
    retryable: false, action: null, detail: "private diagnostic", id: "connection-test" };
  runtime.setCommand("net_connect", async () => { throw error; });
  await user.keyboard("{Enter}");
  const group = await screen.findByRole("group", { name: "Example" });
  const connect = within(group).getByRole("button", { name: translate("net.connect") });
  await tabTo(connect);
  await user.keyboard("{Enter}");
  const alert = await screen.findByRole("alert");
  expect(alert).not.toHaveTextContent("private diagnostic");
  expect(alert.textContent?.length).toBeGreaterThan(0);
  expect(connect).toHaveFocus();
  const report = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(report.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
  await user.keyboard("{Escape}");
  await waitFor(() => expect(trigger).toHaveFocus());
});

for (const auth of ["password", "key", "digest"] as const) {
  it.each(["", "   ", "  päss\t ", "cleared"])(`preserves or retains the ${auth} credential for %j through the save command`, async value => {
    const { runtime, user } = setup("en", {
      auth, protocol: auth === "digest" ? "webdav" : "sftp",
    });
    await user.keyboard("{Enter}");
    const group = await screen.findByRole("group", { name: "Example" });
    await user.click(within(group).getByRole("button", { name: translate("net.edit") }));
    const input = screen.getByLabelText(translate(auth === "key" ? "net.passphrase" : "net.password"));
    expect(input).toHaveValue("");
    expect(input).toHaveAccessibleDescription(translate("net.secretHint"));
    if (value === "cleared") {
      await user.type(input, "temporary");
      await user.clear(input);
    } else if (value) {
      fireEvent.change(input, { target: { value } });
    }
    await user.click(screen.getByRole("button", { name: translate("net.save") }));
    await screen.findByRole("group", { name: "Example" });
    expect(runtime.calls.filter(call => call.cmd === "net_save_profile")).toEqual([
      { cmd: "net_save_profile", args: {
        profile: expect.objectContaining({ id: "p1", auth }),
        secret: value === "" || value === "cleared" ? null : value,
      } },
    ]);
    // Reopening never fills the form with the credential just submitted.
    await user.click(within(screen.getByRole("group", { name: "Example" })).getByRole("button", { name: translate("net.edit") }));
    expect(screen.getByLabelText(translate(auth === "key" ? "net.passphrase" : "net.password"))).toHaveValue("");
  });
}

it.each(["auth", "protocol"] as const)("clears the entered secret when %s changes", async change => {
  const { runtime, user } = setup("en", { auth: "password" });
  await user.keyboard("{Enter}");
  const group = await screen.findByRole("group", { name: "Example" });
  await user.click(within(group).getByRole("button", { name: translate("net.edit") }));
  await user.type(screen.getByLabelText(translate("net.password")), "login-secret");
  await user.selectOptions(screen.getByLabelText(translate(change === "auth" ? "net.auth" : "net.protocol")), change === "auth" ? "key" : "ftps");
  expect(screen.getByLabelText(translate(change === "auth" ? "net.passphrase" : "net.password"))).toHaveValue("");
  await user.click(screen.getByRole("button", { name: translate("net.save") }));
  await waitFor(() => expect(runtime.calls.find(call => call.cmd === "net_save_profile")?.args).toMatchObject({ secret: null }));
});

it.each(["save", "apply"] as const)("blocks duplicate %s calls and releases the guard after failure", async action => {
  const { runtime, user, trigger } = setup("en");
  runtime.setCommand("net_cred_status", () => ({ config: { backend: "keychain", kdbx_path: "", key_file: "", master_in_keychain: false }, unlocked: true, needs_unlock: false }));
  let reject!: (error: unknown) => void;
  const pending = new Promise<void>((_, fail) => { reject = fail; });
  const command = action === "save" ? "net_save_profile" : "net_cred_set_backend";
  runtime.setCommand(command, () => pending);
  await user.click(trigger);
  const group = await screen.findByRole("group", { name: "Example" });
  if (action === "save") {
    await user.click(within(group).getByRole("button", { name: translate("net.edit") }));
  } else {
    await user.click(screen.getByRole("button", { name: translate("cred.manage") }));
  }
  const button = screen.getByRole("button", { name: translate(action === "save" ? "net.save" : "cred.apply") });
  fireEvent.click(button);
  fireEvent.click(button);
  await waitFor(() => expect(runtime.calls.filter(call => call.cmd === command)).toHaveLength(1));
  expect(button).toBeDisabled();
  reject(new Error("test refusal"));
  await waitFor(() => expect(button).toBeEnabled());
  fireEvent.click(button);
  await waitFor(() => expect(runtime.calls.filter(call => call.cmd === command)).toHaveLength(2));
  await waitFor(() => expect(button).toBeEnabled());
});
