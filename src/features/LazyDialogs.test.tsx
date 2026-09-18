// ============================================================
// The lazy dialog boundaries as a set (TICKET-034).
//
// Two properties matter here rather than per-dialog behaviour: at rest
// the application must mount none of them (that is the whole point of
// the split), and every declared loader must actually resolve to the
// component it names — a renamed export would otherwise fail only when
// a user opened that dialog.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { useDemo } from "@/test/tauri";

vi.mock("@/ipc/client", () => ({
  hasTauri: false,
  listDir: () => Promise.resolve({ path: "/", entries: [] }),
  listArchive: () => Promise.reject(new Error("none")),
  homeDir: () => Promise.resolve("/home"),
  requestGitStatus: () => {},
  netListProfiles: () => Promise.resolve([]),
  netListConnections: () => Promise.resolve([]),
  netCredStatus: () =>
    Promise.resolve({ backend: "keychain", needs_unlock: false, unlocked: true }),
  locationCapabilities: () =>
    Promise.resolve({ scheme: "local", store_id: "local", connected: true, caps: [] }),
  sshClose: () => Promise.resolve(),
  volumeInfo: () => Promise.resolve(null),
  dirSize: () => Promise.resolve({ stats: null, cancelled: true }),
  cancelDirSize: () => Promise.resolve(),
  fileProps: () => Promise.reject(new Error("none")),
  getTags: () => Promise.resolve([]),
}));

const { LAZY, PRELOAD_ON_MENU } = await import("./lazyFeatures");
const { LazyDialogs } = await import("./LazyDialogs");
const { useUi } = await import("@/store/uiStore");

beforeEach(() => {
  useDemo();
  vi.useRealTimers();
});

describe("at rest", () => {
  it("mounts no dialog at all", () => {
    const { container } = render(<LazyDialogs />);
    // Not one modal, not one status line: nothing to steal focus or
    // swallow a function key.
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});

describe("opening one", () => {
  it("brings that dialog in and leaves the others out", async () => {
    render(<LazyDialogs />);
    useUi.getState().setAboutOpen(true);
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    // Exactly one, so nothing else was dragged along.
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });
});

describe("the declared boundaries", () => {
  it("every loader resolves to a component", async () => {
    for (const [id, load] of Object.entries(LAZY)) {
      // xterm probes a 1px canvas while its real module is imported. This
      // loader test does not render a terminal. Model an unavailable context
      // (the browser API's null result) instead of jsdom's noisy missing method.
      // Keep the adapter local to this import; other tests retain native APIs.
      const context = id === "terminal"
        ? vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null)
        : null;
      try {
        const mod = await load();
        expect(typeof mod.default, id).toBe("function");
        if (context) expect(context).toHaveBeenCalledWith("2d", { willReadFrequently: true });
      } finally {
        context?.mockRestore();
      }
    }
  });

  it("the preload map only names loaders that exist", () => {
    const known = new Set(Object.values(LAZY));
    for (const [menu, loaders] of Object.entries(PRELOAD_ON_MENU)) {
      for (const load of loaders) {
        expect(known.has(load), `${menu} references an unknown loader`).toBe(true);
      }
    }
  });

  it("warms the heavyweights from the menus that offer them", () => {
    // The three chunks the split exists for must each be reachable by
    // intent, or the fetch always lands in the click instead.
    const warmed = new Set(Object.values(PRELOAD_ON_MENU).flat());
    for (const [name, load] of [
      ["preview", LAZY.preview],
      ["terminal", LAZY.terminal],
      ["settings", LAZY.settings],
      ["connections", LAZY.connections],
    ] as const) {
      expect(warmed.has(load), name).toBe(true);
    }
  });
});
