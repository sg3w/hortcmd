// ============================================================
// Global keyboard isolation (TICKET-033, guarding TICKET-014).
//
// The global handler listens on `window`, above the React root, so it
// sees every keystroke after React has dispatched it. A dialog that
// closes on Enter or Escape unmounts during that very dispatch — by the
// time the global handler runs, `document.activeElement` has fallen
// back to `<body>` and the keystroke looks like one typed in the file
// pane. That is how a confirming Enter used to also open the entry
// underneath. These tests hold that boundary from both sides: what a
// dialog claims never reaches the panes, and what nobody claims still
// does.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import { consumeDialogKey, isDialogKey, markDialogKey } from "@/lib/keys";

const opened: number[] = [];
const deleted: { permanent: boolean }[] = [];

vi.mock("./navigate", () => ({
  openEntry: (_side: string, index: number) => opened.push(index),
}));
vi.mock("./fileOps", () => ({
  runDelete: (_side: string, opts?: { permanent?: boolean }) =>
    deleted.push({ permanent: !!opts?.permanent }),
  runQuickLook: () => false,
}));
vi.mock("@/ipc/client", () => ({
  hasTauri: false,
  listDir: () => Promise.resolve({ path: "/", entries: [] }),
  listArchive: () => Promise.reject(new Error("none")),
  homeDir: () => Promise.resolve("/home"),
  requestGitStatus: () => {},
  locationCapabilities: () =>
    Promise.resolve({
      scheme: "local",
      store_id: "local",
      connected: true,
      caps: ["browse", "stat", "search", "deletePermanent"],
    }),
}));

const { usePanes } = await import("@/store/panesStore");
const { useKeyboard } = await import("./useKeyboard");
const { file } = await import("@/test/fixtures");

/** A component whose only job is to install the global handler. */
function Host() {
  useKeyboard();
  return null;
}

function press(init: KeyboardEventInit & { key: string }, target?: Element) {
  const event = new KeyboardEvent("keydown", {
    bubbles: true,
    cancelable: true,
    ...init,
  });
  (target ?? window).dispatchEvent(event);
  return event;
}

beforeEach(() => {
  opened.length = 0;
  deleted.length = 0;
  const entry = file("a.txt");
  usePanes.setState((s) => ({
    active: "left",
    left: {
      ...s.left,
      tabs: [
        {
          ...s.left.tabs[0],
          path: "/dir",
          raw: [entry],
          entries: [entry],
          cursor: 0,
          filter: "",
        },
      ],
      activeTab: 0,
    },
  }));
  render(<Host />);
});

describe("without a dialog", () => {
  it("acts on the pane", () => {
    press({ key: "Enter" });
    expect(opened).toEqual([0]);
  });

  it("starts the quick filter on a printable character", () => {
    press({ key: "x" });
    expect(usePanes.getState().left.tabs[0].filter).toBe("x");
  });

  it("reads Shift+F8 as a permanent delete", () => {
    press({ key: "F8", shiftKey: true });
    expect(deleted).toEqual([{ permanent: true }]);
  });
});

describe("a keystroke a dialog has claimed", () => {
  it("never reaches the panes, even after the dialog unmounted", () => {
    // Exactly the TICKET-014 sequence: the dialog marks the event while
    // handling it, closes, and the event then continues to `window`
    // with nothing left in the DOM to identify it.
    const event = new KeyboardEvent("keydown", {
      key: "Enter",
      bubbles: true,
      cancelable: true,
    });
    markDialogKey(event);
    window.dispatchEvent(event);
    expect(opened).toEqual([]);
  });

  it("is marked by `consumeDialogKey` too, which also stops it", () => {
    const event = new KeyboardEvent("keydown", {
      key: "Enter",
      bubbles: true,
      cancelable: true,
    });
    consumeDialogKey(event);
    expect(isDialogKey(event)).toBe(true);
    expect(event.defaultPrevented).toBe(true);
    window.dispatchEvent(event);
    expect(opened).toEqual([]);
  });

  it("marks only that one keystroke", () => {
    const claimed = new KeyboardEvent("keydown", { key: "Enter" });
    markDialogKey(claimed);
    const other = new KeyboardEvent("keydown", { key: "Enter" });
    expect(isDialogKey(other)).toBe(false);
  });
});

describe("while a dialog is open", () => {
  it("ignores keystrokes from inside it", () => {
    const { container } = render(
      <div role="dialog">
        <button type="button">ok</button>
      </div>,
    );
    const button = container.querySelector("button")!;
    press({ key: "Enter" }, button);
    expect(opened).toEqual([]);
  });

  it("ignores keystrokes that never reached it either", () => {
    // The dialog may be open but not yet focused (the prompt input
    // focuses one frame later).
    render(<div role="dialog" />);
    press({ key: "Enter" });
    expect(opened).toEqual([]);
  });

  it("lets the panes have the keyboard again once it is gone", () => {
    const view = render(<div role="dialog" />);
    press({ key: "Enter" });
    expect(opened).toEqual([]);
    view.unmount();
    press({ key: "Enter" });
    expect(opened).toEqual([0]);
  });
});

describe("text fields and menus own their keystrokes", () => {
  it("does not intercept typing in an input", () => {
    const { container } = render(<input defaultValue="" />);
    const input = container.querySelector("input")!;
    press({ key: "x" }, input);
    expect(usePanes.getState().left.tabs[0].filter).toBe("");
  });

  it("does not intercept typing in a contenteditable", () => {
    const { container } = render(
      <div contentEditable suppressContentEditableWarning />,
    );
    const el = container.firstElementChild as HTMLElement;
    // jsdom does not implement `isContentEditable`; the handler reads
    // exactly that property, so it is supplied here.
    Object.defineProperty(el, "isContentEditable", { value: true });
    press({ key: "x" }, el);
    expect(usePanes.getState().left.tabs[0].filter).toBe("");
  });

  it("leaves an open menu its arrows and type-ahead", () => {
    const { container } = render(
      <div role="menu" data-state="open">
        <div role="menuitem">x</div>
      </div>,
    );
    press({ key: "x" }, container.querySelector('[role="menuitem"]')!);
    expect(usePanes.getState().left.tabs[0].filter).toBe("");
  });
});
