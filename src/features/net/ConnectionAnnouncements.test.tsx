import { act, render, screen } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import { useDemo } from "@/test/tauri";
import { useSettings } from "@/store/settingsStore";
import type { ConnectionStatus, Phase } from "@/ipc/bindings";
import { useNet } from "./netStore";
import { ConnectionAnnouncements } from "./ConnectionAnnouncements";

beforeEach(() => useDemo());
function status(phase: Phase): ConnectionStatus {
  return { id: "server", key: "server", name: "Build server", address: "sftp://example.invalid", phase,
    connected: phase === "connected", attempt: null, error: null };
}
function publish(phase: Phase) {
  act(() => useNet.setState({ connections: [status(phase)] }));
}

it.each(["de", "en"] as const)("announces lifecycle changes without repeating unchanged polls in %s", async language => {
  useSettings.setState({ language });
  render(<ConnectionAnnouncements />);
  const live = screen.getByRole("status");
  publish("disconnected");
  expect(live).toBeEmptyDOMElement();
  publish("connecting");
  expect(live).toHaveTextContent(language === "de" ? "Verbindung wird aufgebaut" : "Connecting");
  publish("connected");
  expect(live).toHaveTextContent("Build server");
  const mutations: MutationRecord[] = [];
  const observer = new MutationObserver(records => mutations.push(...records));
  observer.observe(live, { childList: true, subtree: true, characterData: true });
  publish("connected");
  await act(async () => { await Promise.resolve(); });
  expect(mutations).toEqual([]);
  observer.disconnect();
  publish("disconnected");
  expect(live).toHaveTextContent(language === "de" ? "Verbindung getrennt" : "Disconnected");
  publish("failed");
  expect(live).toHaveTextContent(language === "de" ? "Verbindung fehlgeschlagen" : "Connection failed");
});
