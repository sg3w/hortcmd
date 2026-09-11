#!/usr/bin/env node
// ============================================================
// Verifies the generated TypeScript bindings are current
// (TICKET-033).
//
// `src/ipc/bindings/` is produced by ts-rs from the Rust types. A
// changed Rust struct with stale bindings type-checks fine and fails at
// runtime, which is exactly the class of mistake CI should catch — so
// the generator runs and the working tree must come out unchanged.
//
// Run as: node scripts/check-bindings.mjs
// ============================================================

import { execFileSync } from "node:child_process";

function run(cmd, args) {
  return execFileSync(cmd, args, { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
}

// Only meaningful inside a git checkout — the comparison is against the index.
try {
  run("git", ["rev-parse", "--is-inside-work-tree"]);
} catch {
  console.error("check-bindings: not a git checkout, nothing to compare against.");
  process.exit(1);
}

const before = run("git", ["status", "--porcelain", "--", "src/ipc/bindings"]).trim();
if (before) {
  console.error(
    "check-bindings: src/ipc/bindings/ already has uncommitted changes:\n" + before,
  );
  process.exit(1);
}

console.log("check-bindings: regenerating from the Rust types …");
run("cargo", [
  "test",
  "--manifest-path",
  "src-tauri/Cargo.toml",
  "--locked",
  "export_bindings",
]);

const after = run("git", ["status", "--porcelain", "--", "src/ipc/bindings"]).trim();
if (after) {
  console.error(
    "check-bindings: the generated bindings are out of date. Run\n" +
      "  npm run gen:types\n" +
      "and commit the result. Changed files:\n" +
      after,
  );
  process.exit(1);
}
console.log("check-bindings: bindings are up to date.");
