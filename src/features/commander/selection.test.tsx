import { beforeEach, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { useDemo } from "@/test/tauri";
import { dir, file, row } from "@/test/fixtures";
import { panelOf, usePanes, type Side, type Row } from "@/store/panesStore";
import { useOps } from "@/store/opsStore";
import { runCommand } from "./commands";
import { promptSelect } from "./selection";
import { useKeyboard } from "./useKeyboard";
import { targetRows } from "./markClipboard";
import { pickRows } from "./exportList";

function Host() { useKeyboard(); return null; }
function seed(side: Side, entries: Row[]) {
  usePanes.setState(s => ({ active: side, [side]: { ...s[side], activeTab: 0,
    tabs: [{ ...s[side].tabs[0], path: "/fixture", raw: entries, entries,
      selected: new Set<string>(), cursor: 1, filter: "" },
      { ...s[side].tabs[0], id: "untouched", selected: new Set(["other.txt"]) }],
  } }));
}
const selected = (side: Side) => panelOf(usePanes.getState(), side).selected;
beforeEach(() => { useDemo(); });

for (const side of ["left", "right"] as const) {
  it.each([[dir("folder")], [file("a.txt")], [dir("folder"), file("a.txt")], []])(
    `${side}: menu and existing shortcuts select the same visible entries: %j`, (...entries) => {
      const rows = [row("..", { is_dir: true, parent: true }), ...entries];
      seed(side, rows);
      const expected = new Set(entries.map(e => e.name));
      runCommand("mark.allFiles");
      expect(selected(side)).toEqual(expected);
      runCommand("mark.invert");
      expect(selected(side).size).toBe(0);
      render(<Host />);
      window.dispatchEvent(new KeyboardEvent("keydown", {
        key: "+", code: "NumpadAdd", ctrlKey: true, bubbles: true,
      }));
      expect(selected(side)).toEqual(expected);
      expect(usePanes.getState()[side].tabs[1].selected).toEqual(new Set(["other.txt"]));
      runCommand("mark.noFiles");
      expect(selected(side).size).toBe(0);
    },
  );
}

it("pattern prompts include folders without an opt-in and preserve mixed snapshots", () => {
  seed("left", [dir("src"), file("src.txt"), file("other.txt")]);
  promptSelect("left", true);
  const prompt = useOps.getState().prompt!;
  expect(prompt.checkbox).toBeUndefined();
  prompt.onSubmit("src*", false);
  expect(selected("left")).toEqual(new Set(["src", "src.txt"]));
  expect(targetRows("left").map(e => e.name)).toEqual(["src", "src.txt"]);
  expect(pickRows(panelOf(usePanes.getState(), "left").entries, selected("left"), {
    onlySelected: true, includeFolders: true,
  }).map(e => e.name)).toEqual(["src", "src.txt"]);
  usePanes.getState().rememberSelection("left");
  usePanes.getState().selectPattern("left", "src*", false);
  expect(selected("left").size).toBe(0);
  usePanes.getState().restoreSelection("left");
  expect(selected("left")).toEqual(new Set(["src", "src.txt"]));
});

it("manual toggle and shift selection still include folders and exclude the parent row", () => {
  seed("left", [row("..", { parent: true, is_dir: true }), dir("folder"), file("a.txt")]);
  usePanes.getState().toggleMark("left", 0);
  expect(selected("left").size).toBe(0);
  usePanes.getState().toggleMark("left", 1, true);
  expect(selected("left")).toEqual(new Set(["folder"]));
  expect(panelOf(usePanes.getState(), "left").cursor).toBe(2);
  usePanes.getState().setCursor("left", 1);
  usePanes.getState().shiftTo("left", 2);
  expect(selected("left")).toEqual(new Set(["folder", "a.txt"]));
});

it("general selection excludes filtered rows; extension selection stays file-specific", () => {
  seed("left", [dir("folder.txt"), file("a.txt"), file("hidden.txt")]);
  usePanes.getState().setFilter("left", "folder");
  runCommand("mark.allFiles");
  expect(selected("left")).toEqual(new Set(["folder.txt"]));
  usePanes.getState().setFilter("left", "");
  usePanes.getState().clearSelection("left");
  usePanes.getState().setCursor("left", panelOf(usePanes.getState(), "left").entries.findIndex(e => e.name === "a.txt"));
  usePanes.getState().selectSameExt("left");
  expect(selected("left")).toEqual(new Set(["a.txt", "hidden.txt"]));
});
