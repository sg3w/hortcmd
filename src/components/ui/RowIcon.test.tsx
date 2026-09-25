import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { RowIcon, iconRequest } from "./RowIcon";
import { row } from "@/test/fixtures";
import { useTauri } from "@/test/tauri";
import { useSettings } from "@/store/settingsStore";
import type { FileIconRequest } from "@/ipc/bindings/FileIconRequest";
vi.mock("@/ipc/invoke", async original => ({ ...await original<typeof import("@/ipc/invoke")>(), hasTauri: () => true }));
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
it("uses path/mtime/theme/scale identities, but type-only requests for remote/archive/links", () => {
  const r = row("a.txt");
  const req = iconRequest(r, "/tmp/a", 14, 2, "dark");
  expect(req.path).toBe("/tmp/a/a.txt"); expect(req.pixels).toBe(28);
  for (const next of [iconRequest(r, "/tmp/b", 14, 2, "dark"), iconRequest({ ...r, modified: 3 }, "/tmp/a", 14, 2, "dark"), iconRequest(r, "/tmp/a", 14, 1, "dark"), iconRequest(r, "/tmp/a", 14, 2, "light")]) expect(next.id).not.toBe(req.id);
  for (const path of [null, "sftp://host/files"]) expect(iconRequest(r, path, 14, 2, "dark").path).toBeNull();
  expect(iconRequest({ ...r, is_symlink: true }, "/tmp", 14, 2, "dark").path).toBeNull();
});
it("simple mode never invokes icons and renders exactly one link overlay, except parent", () => {
  const runtime = useTauri({ commands: {} });
  useSettings.setState({ fileIcons: "simple" });
  const view = render(<RowIcon row={row("link", { is_symlink: true })} />);
  expect(screen.getAllByRole("img", { name: "Symbolischer Link" })).toHaveLength(1);
  view.rerender(<RowIcon row={row("..", { is_symlink: true, parent: true })} />);
  expect(screen.queryByRole("img")).toBeNull();
  expect(runtime.calls).toHaveLength(0);
});
it("ignores late answers after navigation and switching to simple mode", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("IntersectionObserver", class {
    constructor(private callback: (entries: { isIntersecting: boolean }[]) => void) {}
    observe() { this.callback([{ isIntersecting: true }]); } disconnect() {}
  });
  let finish!: (r: { id: string; image: string }[]) => void;
  let sent: FileIconRequest[] = [];
  useTauri({ commands: { file_icons: args => {
    sent = args.requests as FileIconRequest[];
    return new Promise(resolve => { finish = resolve; });
  } } });
  const view = render(<RowIcon row={row("late.txt")} directory="/old" />);
  await act(() => vi.dynamicImportSettled());
  await act(() => vi.advanceTimersByTimeAsync(20));
  expect(sent[0].path).toBe("/old/late.txt");
  view.rerender(<RowIcon row={row("late.txt")} directory="/new" />);
  await act(async () => { finish([{ id: sent[0].id, image: "data:image/png;base64,AA==" }]); });
  expect(view.container.querySelector("img")).toBeNull();
  act(() => useSettings.setState({ fileIcons: "simple" }));
  await act(() => vi.advanceTimersByTimeAsync(20));
  expect(view.container.querySelector("img")).toBeNull();
});
