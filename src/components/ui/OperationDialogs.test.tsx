import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { useDemo } from "@/test/tauri";
import { useOps } from "@/store/opsStore";
import { OperationDialogs } from "./OperationDialogs";

beforeEach(() => useDemo());
it("preserves a follow-up prompt opened by confirmation", async () => {
  const user = userEvent.setup();
  useOps.getState().requestConfirm({
    title: "Confirm", message: "Continue", onConfirm: () => useOps.getState().requestPrompt({
      title: "Follow-up", label: "Name", initial: "retry", onSubmit: vi.fn(),
    }),
  });
  render(<OperationDialogs />);
  await user.click(screen.getByRole("button", { name: "OK" }));
  expect(screen.getByRole("dialog", { name: "Follow-up" })).toBeVisible();
  expect(useOps.getState().prompt?.initial).toBe("retry");
});

it("returns focus to the invoking control after a prompt and follow-up confirmation", async () => {
  const user = userEvent.setup();
  render(<><button onClick={() => useOps.getState().requestPrompt({
    title: "Rename", label: "Name", initial: "old.txt", onSubmit: () => useOps.getState().requestConfirm({
      title: "Result", message: "Finished", onConfirm: vi.fn(),
    }),
  })}>Start</button><OperationDialogs /></>);
  const trigger = screen.getByRole("button", { name: "Start" });
  await user.click(trigger);
  await waitFor(() => expect(screen.getByRole("textbox")).toHaveFocus());
  await user.keyboard("{Enter}");
  await screen.findByRole("dialog", { name: "Result" });
  await user.click(screen.getByRole("button", { name: "OK" }));
  await waitFor(() => expect(trigger).toHaveFocus());
});
