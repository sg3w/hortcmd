import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useDemo } from "@/test/tauri";
import { translate, useSettings } from "@/store/settingsStore";
import { SearchResults } from "./SearchResults";

// A two-row viewport forces navigation to mount previously absent results.
vi.mock("@tanstack/react-virtual", async () => {
  const { useState } = await import("react");
  return { useVirtualizer: ({ count }: { count: number }) => {
    const [start, setStart] = useState(0);
    return {
      getVirtualItems: () => Array.from({ length: Math.min(2, count - start) }, (_, offset) => ({
        key: start + offset, index: start + offset, start: (start + offset) * 40,
      })),
      getTotalSize: () => count * 40,
      scrollToIndex: (index: number) => setStart(Math.min(index, Math.max(0, count - 2))),
    };
  } };
});

it.each(["de", "en"] as const)("navigates beyond the mounted result window in %s", async language => {
  useDemo();
  useSettings.setState({ language });
  const hits = Array.from({ length: 1000 }, (_, index) => ({ name: `${index}.txt`,
    path: `/src/${index}.txt`, is_dir: false, size: 1, modified: 0, detail: "", group: 0 }));
  const reveal = vi.fn();
  render(<SearchResults hits={hits} onReveal={reveal} />);
  const user = userEvent.setup();
  await user.tab();
  const list = screen.getByRole("listbox", { name: translate("search.results") });
  expect(list).toHaveFocus();
  expect(screen.getAllByRole("option")).toHaveLength(2);
  await user.keyboard("{End}");
  const last = screen.getByRole("option", { name: /^999.txt/ });
  expect(last).toHaveAttribute("aria-posinset", "1000");
  expect(last).toHaveAttribute("aria-setsize", "1000");
  expect(list).toHaveAttribute("aria-activedescendant", last.id);
  expect(screen.queryByRole("option", { name: /^0.txt/ })).not.toBeInTheDocument();
  expect(list).toHaveFocus();
  await user.keyboard("{Home}{ArrowDown}{Enter}");
  expect(reveal).toHaveBeenCalledExactlyOnceWith(hits[1]);
});
