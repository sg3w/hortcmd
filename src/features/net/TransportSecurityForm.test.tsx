import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { useSettings } from "@/store/settingsStore";
import { TransportSecurityForm } from "./TransportSecurityForm";

vi.mock("@/ipc/invoke", async original => ({ ...await original<typeof import("@/ipc/invoke")>(), hasTauri: () => true }));
it("requires an explicit global enablement and a separate profile acknowledgment", async () => {
  const runtime = useTauri({ commands: { net_transport_policy: () => ({ allowPlaintext: false }), net_set_transport_policy: () => {} } });
  useSettings.setState({ language: "en" });
  const onChange = vi.fn();
  render(<TransportSecurityForm value={{ plaintextAcknowledged: false, certificate: null }} onChange={onChange} />);
  const user = userEvent.setup();
  await user.click(screen.getByText("Advanced connection security"));
  const [global, profile] = screen.getAllByRole("checkbox");
  await waitFor(() => expect(global).toBeEnabled());
  expect(global).not.toBeChecked(); expect(profile).toBeDisabled();
  await user.click(global);
  await waitFor(() => expect(profile).toBeEnabled());
  expect(onChange).not.toHaveBeenCalled();
  expect(runtime.calls).toContainEqual({ cmd: "net_set_transport_policy", args: { policy: { allowPlaintext: true } } });
  await user.click(profile);
  expect(onChange).toHaveBeenCalledWith({ plaintextAcknowledged: true, certificate: null });
});
