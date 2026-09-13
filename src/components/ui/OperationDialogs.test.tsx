import { render, screen } from "@testing-library/react";
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
