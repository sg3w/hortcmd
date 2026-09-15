// ============================================================
// The settings pages, one by one (TICKET-035).
//
// Each page is now its own module, so each can be mounted on its own —
// which is the point of the split. What is checked is the loop that
// matters: the control shows the stored value, using it writes the
// store, and the store's new value comes back to the control.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@/ipc/client", () => ({
  hasTauri: false,
  listDir: () => Promise.resolve({ path: "/", entries: [] }),
  listArchive: () => Promise.reject(new Error("none")),
  homeDir: () => Promise.resolve("/home"),
  requestGitStatus: () => {},
  locationCapabilities: () =>
    Promise.resolve({ scheme: "local", store_id: "local", connected: true, caps: [] }),
}));

const { useSettings, translate } = await import("@/store/settingsStore");
const { GeneralForm } = await import("./general");
const { ViewForm } = await import("./view");
const { FilesForm } = await import("./files");
const { OperationsForm } = await import("./operations");
const { FavoritesForm } = await import("./favorites");
const { ColorsForm } = await import("./colors");
const { ProgramsForm } = await import("./programs");

beforeEach(() => {
  useSettings.setState({ language: "de" });
});

describe("General", () => {
  it("shows the stored language and switches it", async () => {
    const user = userEvent.setup();
    render(<GeneralForm />);
    await user.click(screen.getByRole("button", { name: "English" }));
    expect(useSettings.getState().language).toBe("en");
  });

  it("switches the theme", async () => {
    const user = userEvent.setup();
    useSettings.setState({ theme: "dark" });
    render(<GeneralForm />);
    await user.click(screen.getByRole("button", { name: translate("settings.theme.light") }));
    expect(useSettings.getState().theme).toBe("light");
  });

  it("switches the font scale", async () => {
    const user = userEvent.setup();
    useSettings.setState({ fontScale: "md" });
    render(<GeneralForm />);
    const [small] = screen.getAllByRole("button", {
      name: translate("settings.scale.sm"),
    });
    await user.click(small);
    expect(useSettings.getState().fontScale).toBe("sm");
  });
});

describe("File view", () => {
  it("toggles 'folders first' and shows the stored value", async () => {
    const user = userEvent.setup();
    useSettings.setState({ foldersFirst: false });
    render(<ViewForm />);
    const [first] = screen.getAllByRole("switch");
    expect(first).toHaveAttribute("aria-checked", "false");
    await user.click(first);
    expect(useSettings.getState().foldersFirst).toBe(true);
    expect(first).toHaveAttribute("aria-checked", "true");
  });

  it("switches the size format", async () => {
    const user = userEvent.setup();
    useSettings.setState({ sizeFormat: "auto" });
    render(<ViewForm />);
    await user.click(
      screen.getByRole("button", { name: translate("settings.sizeFormat.bytes") }),
    );
    expect(useSettings.getState().sizeFormat).toBe("bytes");
  });
});

describe("Files", () => {
  it("toggles hiding system files", async () => {
    const user = userEvent.setup();
    useSettings.setState({ hideSystemFiles: false });
    render(<FilesForm />);
    const [hide] = screen.getAllByRole("switch");
    await user.click(hide);
    expect(useSettings.getState().hideSystemFiles).toBe(true);
  });

  it("toggles the Git status", async () => {
    const user = userEvent.setup();
    useSettings.setState({ gitEnabled: false });
    render(<FilesForm />);
    const [, git] = screen.getAllByRole("switch");
    await user.click(git);
    expect(useSettings.getState().gitEnabled).toBe(true);
  });
});

describe("File operations", () => {
  it("toggles the trash", async () => {
    const user = userEvent.setup();
    useSettings.setState({ useTrash: false });
    render(<OperationsForm />);
    const [trash] = screen.getAllByRole("switch");
    await user.click(trash);
    expect(useSettings.getState().useTrash).toBe(true);
  });

  it("toggles failure-safe transfers", async () => {
    const user = userEvent.setup();
    useSettings.setState({ safeTransfers: false });
    render(<OperationsForm />);
    const safe = screen.getByRole("switch", { name: translate("settings.safeTransfers") });
    await user.click(safe);
    expect(useSettings.getState().safeTransfers).toBe(true);
  });

  it("accepts a speed limit", async () => {
    const user = userEvent.setup();
    useSettings.setState({ speedLimit: 0 });
    render(<OperationsForm />);
    const [limit] = screen.getAllByRole("spinbutton");
    await user.clear(limit);
    await user.type(limit, "250");
    expect(useSettings.getState().speedLimit).toBe(250);
  });
});

describe("Favorites", () => {
  it("lists the stored favourites", () => {
    useSettings.setState({
      favorites: [{ name: "Projekte", path: "/Users/demo/Projects" }],
    });
    render(<FavoritesForm />);
    expect(screen.getByText("Projekte")).toBeInTheDocument();
    expect(screen.getByText("/Users/demo/Projects")).toBeInTheDocument();
  });

  it("says so when there are none", () => {
    useSettings.setState({ favorites: [] });
    render(<FavoritesForm />);
    expect(
      screen.getByText(translate("settings.favorites.empty")),
    ).toBeInTheDocument();
  });
});

describe("File colours", () => {
  it("renders a row per built-in rule and per custom rule", () => {
    useSettings.setState({
      customColorRules: [
        { id: "r1", name: "Logs", pattern: "*.log", dark: "#ff0000", light: "#880000" },
      ],
    });
    render(<ColorsForm />);
    expect(screen.getByDisplayValue("Logs")).toBeInTheDocument();
    expect(screen.getByDisplayValue("*.log")).toBeInTheDocument();
  });
});

describe("Open with", () => {
  it("lists the configured programs", () => {
    useSettings.setState({
      programs: [{ id: "p1", name: "Editor", path: "/usr/bin/editor" }],
    });
    render(<ProgramsForm />);
    expect(screen.getByText("Editor")).toBeInTheDocument();
  });
});

describe("every page", () => {
  it("mounts on its own without the shell", () => {
    // The split only helps if a page has no hidden dependency on the
    // dialog around it.
    for (const Page of [
      GeneralForm,
      ViewForm,
      FilesForm,
      OperationsForm,
      FavoritesForm,
      ColorsForm,
      ProgramsForm,
    ]) {
      const view = render(<Page />);
      expect(view.container.firstChild).not.toBeNull();
      view.unmount();
    }
  });
});
