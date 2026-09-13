import { beforeEach, expect, it, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";
import { useDemo, useTauri } from "@/test/tauri";
import { file } from "@/test/fixtures";
import { usePanes } from "@/store/panesStore";
import { useNotices } from "@/store/noticeStore";
import { useOps } from "@/store/opsStore";
import { changePageZoom, useNativeZoom } from "./useNativeZoom";
import { useKeyboard } from "./useKeyboard";
import { runCommand } from "./commands";

const platform = vi.hoisted(() => ({ mac: false }));
vi.mock("@/ipc/invoke", async importOriginal => ({
  ...await importOriginal<typeof import("@/ipc/invoke")>(),
  get hasTauri() { return "__TAURI_INTERNALS__" in window; },
}));
vi.mock("@/lib/platform", () => ({
  get isMacOS() { return platform.mac; },
  get isWindows() { return !platform.mac; },
  isUnixLike: true,
}));

const command = "plugin:webview|set_webview_zoom";
function Host() {
  useNativeZoom();
  useKeyboard();
  return <div role="dialog"><input aria-label="Test field" /></div>;
}

function press(key: string, extra: KeyboardEventInit = {}, target: EventTarget = window) {
  const event = new KeyboardEvent("keydown", {
    key, code: key === "-" ? "Minus" : "Equal", bubbles: true, cancelable: true,
    ctrlKey: !platform.mac, metaKey: platform.mac, ...extra,
  });
  target.dispatchEvent(event);
  return event;
}

beforeEach(async () => {
  platform.mac = false;
  useTauri({ commands: { [command]: () => undefined } });
  await changePageZoom("reset");
});

for (const mac of [false, true]) {
  it(`zooms modal input and resets through ${mac ? "Cmd" : "Ctrl"}`, async () => {
    platform.mac = mac;
    const native = useTauri({ commands: { [command]: () => undefined } });
    const { getByRole } = render(<Host />);
    const input = getByRole("textbox");
    input.focus();
    const local = vi.fn();
    input.addEventListener("keydown", local);
    for (const key of ["+", "=", "+", "+", "-", "0"]) {
      expect(press(key, {}, input).defaultPrevented).toBe(true);
    }
    await waitFor(() => expect(native.calls.map(c => c.args.value)).toEqual([
      1.25, 1.5, 1.75, 2, 1.75, 1,
    ]));
    expect(local).not.toHaveBeenCalled();
    expect(input).toHaveFocus();
    expect(useOps.getState().prompt).toBeNull();
  });

  it(`does not claim keypad, Alt, composing or other-platform shortcuts (${mac})`, () => {
    platform.mac = mac;
    const native = useTauri({ commands: { [command]: () => undefined } });
    render(<Host />);
    for (const extra of [
      { code: "NumpadAdd" }, { altKey: true }, { isComposing: true },
      { metaKey: !mac, ctrlKey: mac },
    ]) expect(press("+", extra).defaultPrevented).toBe(false);
    expect(native.calls).toEqual([]);
  });
}

it("leaves browser zoom with the host", () => {
  useDemo();
  render(<Host />);
  expect(press("+").defaultPrevented).toBe(false);
});

it("keeps the numeric keypad's native selection behavior", () => {
  useTauri({ commands: { [command]: () => undefined } });
  function Workspace() { useNativeZoom(); useKeyboard(); return null; }
  const entries = [file("a.txt"), file("b.txt")];
  usePanes.setState(s => ({ active: "left", left: { ...s.left, activeTab: 0,
    tabs: [{ ...s.left.tabs[0], entries, raw: entries, selected: new Set<string>() }],
  } }));
  render(<Workspace />);
  expect(press("+", { code: "NumpadAdd" }).defaultPrevented).toBe(true);
  expect(usePanes.getState().left.tabs[0].selected).toEqual(new Set(["a.txt", "b.txt"]));
  expect(useOps.getState().prompt).toBeNull();
});

it("serializes native updates and recovers without adopting a failed scale", async () => {
  let release!: () => void;
  const apply = vi.fn()
    .mockImplementationOnce(() => new Promise<void>(resolve => { release = resolve; }))
    .mockRejectedValueOnce(new Error("unavailable"))
    .mockResolvedValue(undefined);
  const native = useTauri({ commands: { [command]: apply } });
  const first = changePageZoom("in");
  const failed = changePageZoom("in");
  const retry = changePageZoom("in");
  await waitFor(() => expect(apply).toHaveBeenCalledTimes(1));
  release();
  await Promise.all([first, failed, retry]);
  expect(native.calls.map(c => c.args.value)).toEqual([1.25, 1.5, 1.5]);
  expect(useNotices.getState().left).toMatchObject({ key: "zoom.failed", tone: "error" });
});

it("bounds zoom and routes View menu actions to the same native state", async () => {
  const native = useTauri({ commands: { [command]: () => undefined } });
  for (let i = 0; i < 12; i++) await changePageZoom("in");
  expect(native.calls[native.calls.length - 1]?.args.value).toBe(3);
  for (let i = 0; i < 14; i++) await changePageZoom("out");
  expect(native.calls[native.calls.length - 1]?.args.value).toBe(0.5);
  runCommand("view.zoomReset");
  runCommand("view.zoomIn");
  runCommand("view.zoomOut");
  await waitFor(() => expect(native.calls.slice(-3).map(c => c.args.value)).toEqual([1, 1.25, 1]));
});
