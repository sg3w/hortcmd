import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import axe from "axe-core";
import { useTauri } from "@/test/tauri";
import { dir } from "@/test/fixtures";
import { newTab } from "@/store/panes/state";
import { usePanes } from "@/store/panesStore";
import { useSettings } from "@/store/settingsStore";
import { DirTree } from "./DirTree";

// The transport normally snapshots native availability during module loading.
vi.mock("@/ipc/invoke", async importOriginal => ({
  ...await importOriginal<typeof import("@/ipc/invoke")>(),
  hasTauri: true,
}));

// jsdom has no layout. Render every virtual item so keyboard and ARIA tests
// exercise the actual tree, its asynchronous IPC loading and its store state.
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () => Array.from({ length: count }, (_, index) => ({ index, start: index * 22 })),
    getTotalSize: () => count * 22,
    scrollToIndex: vi.fn(),
  }),
}));

beforeEach(() => {
  useTauri({ commands: {
    list_dir: ({ path }) => ({ path, entries: path === "/" ? [dir("alpha"), dir("beta")] : [] }),
  } });
  usePanes.setState(s => ({ left: { ...s.left, tabs: [newTab("/")], activeTab: 0 } }));
});

it.each(["de", "en"] as const)("exposes hierarchy and keyboard navigation in %s", async language => {
  useSettings.setState({ language });
  render(<main><DirTree side="left" /></main>);
  const user = userEvent.setup();
  const tree = screen.getByRole("tree", { name: language === "de" ? "Linker Verzeichnisbaum" : "Left directory tree" });
  await screen.findByRole("treeitem", { name: "beta" });
  const root = screen.getByRole("treeitem", { name: "/" });
  const alpha = screen.getByRole("treeitem", { name: "alpha" });
  act(() => tree.focus());
  await user.keyboard("{ArrowRight}");
  expect(alpha).toHaveAttribute("aria-selected", "true");
  expect(alpha).toHaveAttribute("aria-level", "2");
  expect(alpha).toHaveAttribute("aria-posinset", "1");
  expect(alpha).toHaveAttribute("aria-setsize", "2");
  expect(tree).toHaveAttribute("aria-activedescendant", alpha.id);
  await user.keyboard("{ArrowRight}");
  await waitFor(() => expect(alpha).not.toHaveAttribute("aria-expanded"));
  await user.keyboard("{ArrowRight}");
  expect(alpha).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{ArrowLeft}");
  expect(root).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{End}");
  expect(screen.getByRole("treeitem", { name: "beta" })).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{Home}");
  expect(root).toHaveAttribute("aria-selected", "true");
  expect(tree).toHaveFocus();
  const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(result.violations.filter(v => v.impact === "serious" || v.impact === "critical").map(v => ({ id: v.id, nodes: v.nodes.map(n => n.html) }))).toEqual([]);
});
