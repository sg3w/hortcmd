import { expect, it, vi } from "vitest";
import { useDemo, useTauri } from "@/test/tauri";
it("uses the same typed location in the demo and Tauri adapter", async () => {
  useDemo();
  const demo = await import("./location");
  const expected = await demo.normalizeLocation("sftp://u@h/a%20b", true, "x?y");
  expect(expected.uri).toBe("sftp://u@h/a%2520b/x%3Fy");
  vi.resetModules();
  const native = useTauri({ commands: { vfs_location: () => expected } });
  const client = await import("./location");
  expect(await client.normalizeLocation("sftp://u@h/a%20b", true, "x?y")).toEqual(expected);
  expect(native.calls.filter(call => call.cmd === "vfs_location")).toHaveLength(1);
});
