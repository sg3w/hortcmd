import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useDemo } from "@/test/tauri";
import { MonitorSettingsForm } from "./MonitorSettingsForm";
import { useSettings } from "@/store/settingsStore";

it("exposes accessible per-profile monitoring controls and retains other values", () => {
  useDemo();
  useSettings.setState({ language: "en" });
  const change = vi.fn();
  render(<MonitorSettingsForm onChange={change} />);
  fireEvent.click(screen.getByText("Connection monitoring"));
  fireEvent.click(screen.getByLabelText("Enable monitoring"));
  expect(change).toHaveBeenLastCalledWith({ enabled: false, intervalSeconds: 30, timeoutSeconds: 10, autoReconnect: true, maxAttempts: 3 });
  fireEvent.change(screen.getByLabelText("Response timeout (seconds)"), { target: { value: "15" } });
  expect(change.mock.lastCall?.[0].timeoutSeconds).toBe(15);
});
