// ============================================================
// The context menu against the capability contract
// (TICKET-033, covering TICKET-031).
//
// The menu is the surface where an unsupported action is most likely to
// be offered by accident: it is built from a pane's state and rendered
// long before anyone clicks it. These tests open it over a local folder
// and over each remote protocol and check what is actually enabled —
// the real rendered items, not the helper that decides them.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createRef } from "react";
import type { Capability } from "@/ipc/bindings";
import type { TransKey } from "@/i18n/dictionaries";

let caps: Record<string, Capability[]> = {};

function keyOf(address: string): string {
  const m = /^([a-z][a-z0-9+.-]*):\/\/([^/]*)/i.exec(address);
  return m ? `${m[1].toLowerCase()}://${m[2].toLowerCase()}` : "local";
}

vi.mock("@/ipc/client", () => ({
  hasTauri: () => false,
  listDir: () => Promise.resolve({ path: "/", entries: [] }),
  listArchive: () => Promise.reject(new Error("none")),
  homeDir: () => Promise.resolve("/home"),
  requestGitStatus: () => {},
  openPath: () => Promise.resolve(),
  openTerminal: () => Promise.resolve(),
  locationCapabilities: (address: string) =>
    Promise.resolve({
      scheme: "x",
      store_id: keyOf(address),
      connected: true,
      caps: caps[keyOf(address)] ?? [],
    }),
}));

const LOCAL: Capability[] = [
  "browse",
  "preview",
  "copyFrom",
  "copyTo",
  "move",
  "create",
  "deletePermanent",
  "trash",
  "archivePackFrom",
  "archiveExtractFrom",
  "stat",
  "systemOpen",
  "openWith",
  "localTerminal",
];
const REMOTE_CORE: Capability[] = [
  "browse",
  "preview",
  "copyFrom",
  "copyTo",
  "move",
  "create",
  "deletePermanent",
];

const { usePanes } = await import("@/store/panesStore");
const { translate } = await import("@/store/settingsStore");
const { loadCaps, useCaps } = await import("@/store/capsStore");
const { FileContextMenu } = await import("./FileContextMenu");
const { file } = await import("@/test/fixtures");

async function openMenuOver(path: string, capsFor: Capability[]) {
  caps = { [keyOf(path)]: capsFor };
  useCaps.getState().invalidateAll();
  await loadCaps(path);

  const entry = file("a.txt");
  usePanes.setState((s) => ({
    left: {
      ...s.left,
      tabs: [
        {
          ...s.left.tabs[0],
          path,
          archive: null,
          raw: [entry],
          entries: [entry],
          cursor: 0,
        },
      ],
      activeTab: 0,
    },
  }));

  const targetRef = createRef<number | null>();
  render(
    <FileContextMenu side="left" targetRef={targetRef}>
      <div data-testid="pane">Dateien</div>
    </FileContextMenu>,
  );
  const user = userEvent.setup();
  await user.pointer({ keys: "[MouseRight]", target: screen.getByTestId("pane") });
  await screen.findByRole("menu");
}

/**
 * Whether the item for this translation key is offered as usable. Keyed
 * on the key, not the wording, so rephrasing a label does not break a
 * test about capabilities.
 */
function enabled(key: TransKey): boolean {
  const label = translate(key);
  const item = screen
    .getAllByText(label)
    .map((el) => el.closest('[role^="menuitem"]'))
    .find(Boolean);
  if (!item) throw new Error(`no menu item for "${key}" (${label})`);
  return item.getAttribute("data-disabled") === null;
}

beforeEach(() => {
  caps = {};
});

describe("over a local folder", () => {
  it("offers the local actions", async () => {
    await openMenuOver("/dir", LOCAL);
    for (const key of [
      "ctx.rename",
      "ctx.view",
      "ctx.mkdir",
      "ctx.terminal",
      "ctx.openFolder",
      "ctx.props",
      "ctx.delete",
      "ctx.pack",
      "ctx.edit",
    ] as const) {
      expect(enabled(key), key).toBe(true);
    }
  });
});

describe.each([
  ["SFTP", "sftp://u@host:22/srv", [...REMOTE_CORE, "sshTerminal" as Capability]],
  ["FTP", "ftp://u@files:21/pub", REMOTE_CORE],
  ["SMB", "smb://u@nas:445/share", REMOTE_CORE],
  ["WebDAV", "webdav://u@dav:443/remote", REMOTE_CORE],
])("over a %s folder", (name, path, protocolCaps) => {
  it("keeps the operations the VFS really implements", async () => {
    await openMenuOver(path, protocolCaps);
    for (const key of [
      "ctx.rename",
      "ctx.view",
      "ctx.mkdir",
      "ctx.delete",
    ] as const) {
      expect(enabled(key), key).toBe(true);
    }
  });

  it("disables every local-only action", async () => {
    await openMenuOver(path, protocolCaps);
    for (const key of [
      "ctx.openFolder",
      "ctx.props",
      "ctx.pack",
      "ctx.extract",
      "ctx.edit",
    ] as const) {
      expect(enabled(key), key).toBe(false);
    }
  });

  it(`offers a terminal only where the connection carries a shell (${name})`, async () => {
    await openMenuOver(path, protocolCaps);
    expect(enabled("ctx.terminal")).toBe(name === "SFTP");
  });
});

describe("over a folder whose connection is gone", () => {
  it("offers nothing that would touch it", async () => {
    await openMenuOver("sftp://u@dead:22/srv", []);
    for (const key of [
      "ctx.rename",
      "ctx.view",
      "ctx.mkdir",
      "ctx.delete",
      "ctx.terminal",
      "ctx.props",
    ] as const) {
      expect(enabled(key), key).toBe(false);
    }
  });
});
