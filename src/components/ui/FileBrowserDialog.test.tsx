import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import axe from "axe-core";
import { useTauri } from "@/test/tauri";
import { dir, file } from "@/test/fixtures";
import { translate, useSettings } from "@/store/settingsStore";
import { openFileBrowser } from "@/store/fileBrowserStore";
import { FileBrowserDialog } from "./FileBrowserDialog";

vi.mock("@/ipc/invoke", async importOriginal => ({
  ...await importOriginal<typeof import("@/ipc/invoke")>(), hasTauri: true,
}));

for (const mode of ["file", "folder"] as const) {
  it.each(["de", "en"] as const)(`picks a ${mode} through keyboard navigation in %s`, async language => {
    useTauri({ commands: {
      list_drives: () => [],
      list_dir: ({ path }) => ({ path, entries: path === "/src" ? [dir("nested"), file("other.txt")] : [file("chosen.txt")] }),
    } });
    useSettings.setState({ language });
    const picked = vi.fn();
    render(<><button onClick={() => openFileBrowser({ selectMode: mode, initialPath: "/src", onPick: picked })}>Start</button><FileBrowserDialog /></>);
    const user = userEvent.setup();
    await user.tab();
    const trigger = screen.getByRole("button", { name: "Start" });
    await user.keyboard("{Enter}");
    await screen.findByRole("option", { name: "nested" });
    const list = screen.getByRole("listbox", { name: translate("fb.title") });
    for (let i = 0; i < 10 && document.activeElement !== list; i++) await user.tab();
    expect(list).toHaveFocus();
    await user.keyboard("{Home}{ArrowDown}");
    expect(list).toHaveAttribute("aria-activedescendant", screen.getByRole("option", { name: "nested" }).id);
    if (mode === "file") {
      expect(screen.getByRole("button", { name: translate("fb.select") })).toBeDisabled();
      await user.keyboard("{Enter}");
      await screen.findByRole("option", { name: /chosen.txt/ });
      await user.keyboard("{End}");
      expect(screen.getByRole("option", { name: /chosen.txt/ })).toHaveAttribute("aria-selected", "true");
    } else {
      expect(screen.getByRole("option", { name: "nested" })).toHaveAttribute("aria-selected", "true");
    }
    const report = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
    expect(report.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
    if (mode === "folder") await user.keyboard("{Tab}{Tab}");
    await user.keyboard("{Enter}");
    expect(picked).toHaveBeenCalledExactlyOnceWith(mode === "file" ? "/src/nested/chosen.txt" : "/src/nested");
    await waitFor(() => expect(trigger).toHaveFocus());
  });
}
