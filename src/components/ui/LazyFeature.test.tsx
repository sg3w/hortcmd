// ============================================================
// Lazy feature boundaries (TICKET-034).
//
// Three states matter and all three are easy to get wrong: nothing must
// exist while the feature is closed (an invisible modal would swallow
// keys and focus), the loading indicator must be inert, and a rejected
// import must be recoverable rather than taking the application tree
// with it.
// ============================================================

import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { LazyFeature, preload, type Loader } from "./LazyFeature";
import { translate } from "@/store/settingsStore";

/** A loader that resolves only when the test says so. */
function deferred() {
  let release!: () => void;
  const gate = new Promise<void>((r) => (release = r));
  const load: Loader = () =>
    gate.then(() => ({
      default: () => <div data-testid="feature">geladen</div>,
    }));
  return { load, release };
}

const ok: Loader = () =>
  Promise.resolve({ default: () => <div data-testid="feature">geladen</div> });

const fails: Loader = () => Promise.reject(new Error("chunk weg"));

describe("while closed", () => {
  it("renders nothing at all", () => {
    const { container } = render(<LazyFeature open={false} load={ok} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("does not import the chunk", () => {
    const load = vi.fn(ok);
    render(<LazyFeature open={false} load={load} />);
    expect(load).not.toHaveBeenCalled();
  });
});

describe("while loading", () => {
  it("shows an announced, non-modal status line", async () => {
    const { load, release } = deferred();
    render(<LazyFeature open load={load} label="Suche" />);

    const status = await screen.findByRole("status");
    expect(status).toHaveTextContent(translate("lazy.loading"));
    expect(status).toHaveTextContent("Suche");
    // Inert: no focus trap, no pointer capture, nothing to click.
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status.className).toContain("pointer-events-none");
    expect(status.querySelector("button")).toBeNull();
    // It must not take focus away from the panes.
    expect(status.contains(document.activeElement)).toBe(false);

    release();
    await screen.findByTestId("feature");
  });

  it("accepts a caller-supplied fallback instead", async () => {
    const { load, release } = deferred();
    render(<LazyFeature open load={load} fallback={<span>eigener</span>} />);
    expect(await screen.findByText("eigener")).toBeInTheDocument();
    release();
    await screen.findByTestId("feature");
  });
});

describe("on success", () => {
  it("mounts the feature", async () => {
    render(<LazyFeature open load={ok} />);
    expect(await screen.findByTestId("feature")).toBeInTheDocument();
  });

  it("imports only once across a reopen", async () => {
    const load = vi.fn(ok);
    const view = render(<LazyFeature open load={load} />);
    await screen.findByTestId("feature");

    view.rerender(<LazyFeature open={false} load={load} />);
    view.rerender(<LazyFeature open load={load} />);
    await screen.findByTestId("feature");
    expect(load).toHaveBeenCalledTimes(1);
  });
});

describe("on a rejected import", () => {
  it("reports it and keeps the shell alive", async () => {
    // React logs the boundary's error; that is expected here.
    const quiet = vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <div>
        <span data-testid="shell">Panes</span>
        <LazyFeature open load={fails} label="Terminal" />
      </div>,
    );

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(translate("lazy.failed"));
    expect(alert).toHaveTextContent("Terminal");
    // The rest of the application is still there.
    expect(screen.getByTestId("shell")).toBeInTheDocument();
    quiet.mockRestore();
  });

  it("offers a retry that really re-imports", async () => {
    const quiet = vi.spyOn(console, "error").mockImplementation(() => {});
    let attempt = 0;
    const flaky: Loader = () => {
      attempt += 1;
      return attempt === 1
        ? Promise.reject(new Error("chunk weg"))
        : Promise.resolve({
            default: () => <div data-testid="feature">geladen</div>,
          });
    };

    render(<LazyFeature open load={flaky} />);
    const retry = await screen.findByRole("button", {
      name: translate("lazy.retry"),
    });
    await userEvent.click(retry);

    // A cached `lazy()` would replay the rejection forever.
    await waitFor(() => expect(screen.getByTestId("feature")).toBeInTheDocument());
    expect(attempt).toBe(2);
    quiet.mockRestore();
  });
});

describe("preload", () => {
  it("fetches the chunk without rendering anything", async () => {
    const load = vi.fn(ok);
    preload(load);
    expect(load).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("feature")).not.toBeInTheDocument();
  });

  it("swallows a failed warm-up — the real attempt reports instead", () => {
    expect(() => preload(fails)).not.toThrow();
  });
});
