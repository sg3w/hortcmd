// ============================================================
// The shared dialog shell (TICKET-033).
//
// Every dialog in the app is this component, so its contract is the
// app's dialog contract: it mounts only when open, it traps and
// restores focus, Escape closes it unless the content needs the key
// itself, and — the reason TICKET-014 exists — a keystroke handled here
// is marked so the global pane handler leaves it alone.
// ============================================================

import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { isDialogKey } from "@/lib/keys";
import { AppDialog } from "./AppDialog";

function open(props: Partial<Parameters<typeof AppDialog>[0]> = {}) {
  const onClose = vi.fn();
  const view = render(
    <AppDialog
      dialogKey="test"
      open
      onClose={onClose}
      titleBar={<span>Titel</span>}
      defaultSize={{ w: 400, h: 300 }}
      {...props}
    >
      <button type="button">Inhalt</button>
    </AppDialog>,
  );
  return { onClose, view };
}

describe("mounting", () => {
  it("renders nothing while closed", () => {
    render(
      <AppDialog
        dialogKey="test"
        open={false}
        onClose={() => {}}
        titleBar={<span>Titel</span>}
        defaultSize={{ w: 400, h: 300 }}
      >
        <button type="button">Inhalt</button>
      </AppDialog>,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows its title bar and content when open", () => {
    open();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("Titel")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Inhalt" })).toBeInTheDocument();
  });

  it("renders a footer when it has one", () => {
    open({ footer: <span>Fußzeile</span> });
    expect(screen.getByText("Fußzeile")).toBeInTheDocument();
  });
});

describe("keyboard", () => {
  it("closes on Escape", async () => {
    const user = userEvent.setup();
    const { onClose } = open();
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("keeps Escape inside when the content needs it", async () => {
    // The SSH terminal (ticket 026) sends Escape to the remote shell.
    const user = userEvent.setup();
    const { onClose } = open({ keepEscape: true });
    await user.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
  });

  it("marks its keystrokes so the pane handler ignores them", async () => {
    // This is the TICKET-014 guarantee: the mark is set on capture,
    // before anything inside the dialog can close it.
    const user = userEvent.setup();
    let marked: boolean | null = null;
    window.addEventListener(
      "keydown",
      (e) => {
        marked = isDialogKey(e);
      },
      { once: true },
    );
    open();
    screen.getByRole("button", { name: "Inhalt" }).focus();
    await user.keyboard("{Enter}");
    expect(marked).toBe(true);
  });
});

describe("focus", () => {
  it("moves focus into the dialog and restores it on close", async () => {
    const outside = document.createElement("button");
    outside.textContent = "außen";
    document.body.appendChild(outside);
    outside.focus();
    expect(document.activeElement).toBe(outside);

    const shell = (isOpen: boolean) => (
      <AppDialog
        dialogKey="test"
        open={isOpen}
        onClose={() => {}}
        titleBar={<span>Titel</span>}
        defaultSize={{ w: 400, h: 300 }}
      >
        <button type="button">Inhalt</button>
      </AppDialog>
    );
    const view = render(shell(true));

    // Radix moves focus into the content when the dialog mounts.
    await vi.waitFor(() =>
      expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(
        true,
      ),
    );

    // Closing removes the dialog and with it the focus trap. (Where
    // focus *lands* afterwards is Radix's own contract and behaves
    // differently under jsdom, so it is not asserted here.)
    view.rerender(shell(false));
    await vi.waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    outside.focus();
    expect(document.activeElement).toBe(outside);
    outside.remove();
  });
});
