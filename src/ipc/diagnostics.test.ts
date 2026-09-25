import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
it("traces action results without transmitting parameters and tolerates logging failure", async () => {
  const h=useTauri({commands:{ diagnostics_event: () => {throw new Error("disk full");}, copy_entries: () => {throw {code:"fs.notFound",detail:"CANARY"};} }});
  vi.resetModules(); const {invoke}=await import("./invoke");
  await expect(invoke("copy_entries",{source:"CANARY",password:"CANARY"})).rejects.toMatchObject({code:"fs.notFound"});
  const events=h.calls.filter(c=>c.cmd==="diagnostics_event");
  expect(events.map(c=>c.args.event)).toEqual(["started","failed"]);
  expect(events[0].args.correlation).toBe(events[1].args.correlation);
  expect(JSON.stringify(events)).not.toContain("CANARY");
});
