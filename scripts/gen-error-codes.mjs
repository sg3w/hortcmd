#!/usr/bin/env node
// ============================================================
// Mirrors the backend's error codes into TypeScript (TICKET-036).
//
// The codes are declared once, in Rust. This copies them so the
// translation test can walk the real list — a hand-kept copy would
// drift, and a missing translation would then show a generic message
// with nobody noticing.
//
//   node scripts/gen-error-codes.mjs           # write
//   node scripts/gen-error-codes.mjs --check   # fail if out of date
// ============================================================

import { readFileSync, writeFileSync } from "node:fs";

const SOURCE = "src-tauri/src/errors/mod.rs";
const TARGET = "src/lib/errorCodes.generated.ts";

const codes = [
  ...readFileSync(SOURCE, "utf8").matchAll(
    /pub const [A-Z_]+: &str = "([^"]+)";/g,
  ),
].map((m) => m[1]);

if (codes.length === 0) {
  console.error(`gen-error-codes: no codes found in ${SOURCE}`);
  process.exit(1);
}

const content =
  `// ============================================================
// Every error code the backend can send (TICKET-036).
//
// Generated from \`${SOURCE}\` by
// \`scripts/gen-error-codes.mjs\` and checked in, so the translation test
// can walk the real list rather than a hand-kept copy. \`npm run check\`
// regenerates it and fails if this file is out of date — the same rule
// the ts-rs bindings follow.
// ============================================================

export const ERROR_CODES = [
` +
  codes.map((c) => `  "${c}",\n`).join("") +
  `] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];
`;

if (process.argv.includes("--check")) {
  const current = readFileSync(TARGET, "utf8");
  if (current !== content) {
    console.error(
      `gen-error-codes: ${TARGET} is out of date. Run\n` +
        "  node scripts/gen-error-codes.mjs\n" +
        "and commit the result.",
    );
    process.exit(1);
  }
  console.log(`gen-error-codes: ${codes.length} codes, up to date.`);
} else {
  writeFileSync(TARGET, content);
  console.log(`gen-error-codes: wrote ${codes.length} codes to ${TARGET}.`);
}
