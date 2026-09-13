import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import axe from "axe-core";
import { useDemo } from "@/test/tauri";
import { row } from "@/test/fixtures";
import { newTab } from "@/store/panes/state";
import { usePanes } from "@/store/panesStore";
import { useSettings } from "@/store/settingsStore";
import { FileTable } from "./FileTable";

// Render a window beginning after two omitted rows, as a scrolled list does.
// Layout/scroll measurement belongs to browser checks; semantics are real here.
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () => Array.from({ length: Math.max(0, count - 2) }, (_, i) => ({ index: i + 2, start: (i + 2) * 24 })),
    getTotalSize: () => count * 24,
    scrollToIndex: vi.fn(),
    measure: vi.fn(),
  }),
}));

beforeEach(() => {
  useDemo();
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  usePanes.setState(s => ({ right: { ...s.right, activeTab: 0, tabs: [{
    ...newTab("/"), entries: [row("a.txt"), row("b.txt"), row("c.txt"), row("d.txt")],
    cursor: 2, selected: new Set(["d.txt"]),
  }] } }));
});
afterEach(() => vi.unstubAllGlobals());

for (const path of ["/fixture", "sftp://user@example.org/fixture", "inside"]) {
  it.each(["details", "list", "thumbnails", "tree"] as const)(
    `renders general folder marks in %s at ${path}`, viewMode => {
      const entries = [row("..", { parent: true, is_dir: true }), row("a.txt"), row("folder", { is_dir: true }), row("b.txt")];
      usePanes.setState(s => ({ right: { ...s.right, viewMode, tabs: [{
        ...newTab(path), archive: path === "inside" ? "/fixture.zip" : null,
        raw: entries, entries,
      }] } }));
      render(<FileTable side="right" onOpen={() => {}} />);
      act(() => usePanes.getState().selectAllFiles("right"));
      expect(document.getElementById("right-entry-2")).toHaveAttribute("aria-selected", "true");
      expect(document.getElementById("right-entry-3")).toHaveAttribute("aria-selected", "true");
      act(() => usePanes.getState().invertSelection("right"));
      expect(document.getElementById("right-entry-2")).toHaveAttribute("aria-selected", "false");
      expect(document.getElementById("right-entry-3")).toHaveAttribute("aria-selected", "false");
    },
  );
}

for (const language of ["de", "en"] as const) {
  it.each(["details", "list", "thumbnails"] as const)(`exposes virtual positions and independent marks in ${language} %s`, async viewMode => {
    useSettings.setState({ language });
    usePanes.setState(s => ({ right: { ...s.right, viewMode } }));
    render(<main><FileTable side="right" onOpen={vi.fn()} /></main>);
    const grid = screen.getByRole("grid", { name: language === "de" ? "Rechte Dateiliste" : "Right file list" });
    expect(grid).toHaveAttribute("aria-rowcount", viewMode === "details" ? "5" : "4");
    expect(grid).toHaveAttribute("aria-activedescendant", "right-entry-2");
    expect(document.getElementById("right-entry-2")).toHaveAttribute("aria-selected", "false");
    expect(document.getElementById("right-entry-3")).toHaveAttribute("aria-selected", "true");
    const renderedRows = screen.getAllByRole("row");
    expect(renderedRows[viewMode === "details" ? 1 : 0]).toHaveAttribute("aria-rowindex", viewMode === "details" ? "4" : "3");
    act(() => grid.focus());
    expect(usePanes.getState().active).toBe("right");
    if (viewMode === "details") {
      const user = userEvent.setup();
      const sortButton = screen.getByRole("button", { name: language === "de" ? "Größe" : "Size" });
      act(() => sortButton.focus());
      await user.keyboard("{Enter}");
      expect(usePanes.getState().right.tabs[0].sort.key).toBe("size");
      const resize = screen.getByRole("separator", { name: /: (Größe|Size)$/ });
      const originalWidth = Number(resize.getAttribute("aria-valuenow"));
      act(() => resize.focus());
      await user.keyboard("{ArrowRight}");
      expect(resize).toHaveAttribute("aria-valuenow", String(originalWidth + 10));
      await user.keyboard("{Home}");
      expect(resize).toHaveAttribute("aria-valuenow", "40");
      expect(sortButton.closest('[role="columnheader"]')).toHaveAttribute("aria-sort", "ascending");
    }
    const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
    expect(result.violations.filter(v => v.impact === "serious" || v.impact === "critical").map(v => ({ id: v.id, nodes: v.nodes.map(n => n.html) }))).toEqual([]);
  });
}
