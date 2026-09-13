import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import axe from "axe-core";
import { useDemo } from "@/test/tauri";
import { translate, useSettings } from "@/store/settingsStore";
import { useOps } from "@/store/opsStore";
import { openProps } from "@/store/propsStore";
import { openFileBrowser } from "@/store/fileBrowserStore";
import { PreviewDialog } from "./PreviewDialog";
import { PropertiesDialog } from "./PropertiesDialog";
import { FileBrowserDialog } from "./FileBrowserDialog";

const dialogs = [
  { name: "preview", Component: PreviewDialog, open: () => useOps.getState().setPreview({
    kind: "text", name: "example.txt", size: 4, text: "test", hex: null, data_url: null,
    exif: [], truncated: false,
  }) },
  { name: "properties", Component: PropertiesDialog, open: () => openProps("/Users/demo/example.txt") },
  { name: "file browser", Component: FileBrowserDialog, open: () => openFileBrowser({
    selectMode: "file", initialPath: "/Users/demo", onPick: () => {},
  }) },
];

for (const { name, Component, open } of dialogs) {
  it.each(["de", "en"] as const)(`${name} returns keyboard focus after Escape in %s`, async language => {
    useDemo();
    useSettings.setState({ language });
    render(<><button onClick={open}>Start</button><Component /></>);
    const user = userEvent.setup();
    await user.tab();
    const trigger = screen.getByRole("button", { name: "Start" });
    expect(trigger).toHaveFocus();
    await user.keyboard("{Enter}");
    const dialog = await screen.findByRole("dialog");
    await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
    if (name === "preview") {
      expect(screen.getByRole("button", { name: translate("op.close") })).toBeVisible();
      const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
      expect(result.violations.filter(v => v.impact === "serious" || v.impact === "critical")).toEqual([]);
    }
    await user.keyboard("{Escape}");
    await waitFor(() => expect(trigger).toHaveFocus());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
}
