#!/usr/bin/env node
// ============================================================
// Verifies the generated TypeScript bindings are current
// (TICKET-033).
//
// `src/ipc/bindings/` is produced by ts-rs from the Rust types. A
// changed Rust struct with stale bindings type-checks fine and fails at
// runtime, which is exactly the class of mistake CI should catch.
//
// The check snapshots the directory, regenerates, and compares the
// contents. Deliberately *not* `git status`: that would also fail on a
// branch where the bindings are legitimately being changed — which is
// precisely when the check matters most.
//
//   node scripts/check-bindings.mjs
// ============================================================

import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const DIR = "src/ipc/bindings";

function snapshot() {
  if (!existsSync(DIR)) return new Map();
  return new Map(
    readdirSync(DIR)
      .filter((f) => f.endsWith(".ts"))
      .map((f) => [f, readFileSync(join(DIR, f), "utf8")]),
  );
}

const before = snapshot();

console.log("check-bindings: regenerating from the Rust types …");
execFileSync(
  "cargo",
  ["test", "--manifest-path", "src-tauri/Cargo.toml", "--locked", "export_bindings"],
  { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] },
);

const after = snapshot();

const changed = [];
for (const [file, content] of after) {
  if (!before.has(file)) changed.push(`${file} (new)`);
  else if (before.get(file) !== content) changed.push(`${file} (changed)`);
}
// A file the generator no longer writes is stale and must be deleted by
// hand — the generator cannot know it is gone.
for (const file of before.keys()) {
  if (!after.has(file)) changed.push(`${file} (no longer generated)`);
}

if (changed.length) {
  // Put the regenerated content back is unnecessary — it is already
  // written; the working tree now holds the correct bindings.
  console.error(
    "check-bindings: the bindings were out of date and have been regenerated.\n" +
      "Commit the result. Changed files:\n" +
      changed.map((c) => `  ${c}`).join("\n"),
  );
  process.exit(1);
}

console.log(`check-bindings: ${after.size} bindings are up to date.`);
