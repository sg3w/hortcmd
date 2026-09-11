// ============================================================
// The browser demo's capability set (TICKET-031).
//
// Without a Tauri backend the app still has to enable and disable the
// same things, so the demo answer is deterministic and mirrors the
// local matrix of `vfs::caps` — including its platform gates.
// ============================================================

import { describe, expect, it } from "vitest";
import { locationCapabilities } from "./client";
import { isMacOS, isUnixLike } from "@/lib/platform";

describe("demo capabilities", () => {
  it("answers the same set for every address", async () => {
    const a = await locationCapabilities("/Users/demo");
    const b = await locationCapabilities("sftp://u@host/srv");
    expect(a.store_id).toBe("local");
    expect(a.connected).toBe(true);
    expect(b.caps).toEqual(a.caps);
  });

  it("carries the local matrix", async () => {
    const { caps } = await locationCapabilities("/Users/demo");
    for (const cap of [
      "browse",
      "preview",
      "copyFrom",
      "copyTo",
      "move",
      "create",
      "deletePermanent",
      "trash",
      "archiveBrowse",
      "search",
      "compareDirs",
      "gitStatus",
      "volumeInfo",
      "systemOpen",
      "openWith",
      "localTerminal",
    ] as const) {
      expect(caps, cap).toContain(cap);
    }
    // Never a shell on a connection the demo does not have.
    expect(caps).not.toContain("sshTerminal");
  });

  it("applies the same platform gates as the backend", async () => {
    const { caps } = await locationCapabilities("/Users/demo");
    expect(caps.includes("permissions")).toBe(isUnixLike);
    expect(caps.includes("quickLook")).toBe(isMacOS);
    expect(caps.includes("finderTags")).toBe(isMacOS);
  });
});
