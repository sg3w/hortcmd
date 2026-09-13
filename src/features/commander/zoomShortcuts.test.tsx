import { beforeEach, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { useDemo } from "@/test/tauri";
import { file } from "@/test/fixtures";
import { usePanes } from "@/store/panesStore";
import { useOps } from "@/store/opsStore";
import { translate } from "@/store/settingsStore";
import { useKeyboard } from "./useKeyboard";

function Host() {
  useKeyboard();
  return null;
}

beforeEach(() => {
  useDemo();
  const entries = [file("a.txt"), file("b.txt")];
  usePanes.setState(s => ({ active: "left", left: { ...s.left, activeTab: 0,
    tabs: [{ ...s.left.tabs[0], path: "/fixture", raw: entries, entries,
      cursor: 0, filter: "", selected: new Set(["a.txt"]) }],
  } }));
  render(<Host />);
});

function press(key: string, code: string, modifiers: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", {
    key, code, bubbles: true, cancelable: true, ...modifiers,
  });
  window.dispatchEvent(event);
  return event;
}

for (const modifier of ["ctrlKey", "metaKey"] as const) {
  it.each([["+", "Equal"], ["=", "Equal"], ["-", "Minus"], ["0", "Digit0"]])(
    `leaves ${modifier} + %s available to host zoom`, (key, code) => {
      const event = press(key, code, { [modifier]: true, shiftKey: key === "+" });
      expect(useOps.getState().prompt).toBeNull();
      expect(event.defaultPrevented).toBe(false);
      expect(usePanes.getState().left.tabs[0].selected).toEqual(new Set(["a.txt"]));
      expect(usePanes.getState().left.tabs[0].filter).toBe("");
    },
  );

  it(`preserves ${modifier} numeric-keypad selection commands`, () => {
    expect(press("+", "NumpadAdd", { [modifier]: true }).defaultPrevented).toBe(true);
    expect(usePanes.getState().left.tabs[0].selected).toEqual(new Set(["a.txt", "b.txt"]));
    expect(press("-", "NumpadSubtract", { [modifier]: true }).defaultPrevented).toBe(true);
    expect(usePanes.getState().left.tabs[0].selected.size).toBe(0);
    expect(useOps.getState().prompt).toBeNull();
  });
}

it.each([["+", "Equal", "sel.select.title"], ["-", "Minus", "sel.unselect.title"]] as const)(
  "preserves the unmodified %s selection prompt", (key, code, title) => {
    expect(press(key, code).defaultPrevented).toBe(true);
    expect(useOps.getState().prompt?.title).toBe(translate(title));
  },
);
