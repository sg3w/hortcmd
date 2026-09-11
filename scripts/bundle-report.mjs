#!/usr/bin/env node
// ============================================================
// Bundle composition and the eager-chunk budget (TICKET-034).
//
// The point of splitting is that the WebView parses as little as
// possible before it can draw two file panes. That property is easy to
// lose by accident — one static import of a dialog from a store, and a
// 150 kB chunk is back in the entry graph with nothing visibly wrong.
//
// So the budget is checked, not hoped for: this reads the build's
// manifest, reports which major dependency ended up in which chunk, and
// fails when the eager entry chunk exceeds the documented budget.
//
//   node scripts/bundle-report.mjs          # build must have run
//   node scripts/bundle-report.mjs --build  # build first
// ============================================================

import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const DIST = "dist";
const MANIFEST = join(DIST, ".vite", "manifest.json");

/** Kept in step with vite.config.ts, which is the documented home. */
const EAGER_BUDGET_KB = Number(
  /EAGER_BUDGET_KB = (\d+)/.exec(readFileSync("vite.config.ts", "utf8"))?.[1],
);
if (!Number.isFinite(EAGER_BUDGET_KB)) {
  console.error("bundle-report: could not read EAGER_BUDGET_KB from vite.config.ts");
  process.exit(1);
}

/**
 * Dependencies whose placement is the point of the split, detected by a
 * token that only that library emits. Package *names* are useless here:
 * minified output keeps almost none of them, while ordinary UI strings
 * ("… marked", "react to") match them by accident.
 */
const WATCHED = [
  ["xterm", /xterm-(?:screen|rows|viewport|dom-renderer|cursor-layer)/],
  ["highlight.js", /\bhljs\b/],
  ["marked", /marked\.js\.org/],
  ["dompurify", /DOMPurify/],
  ["react", /react-dom-client|react\.element|react\.fragment/],
  ["radix", /radix-(?:ui|collection-item)|data-radix/],
  ["tanstack", /getCoreRowModel|@tanstack/],
  ["lucide", /"lucide-react"|lucide-react/],
];

if (process.argv.includes("--build")) {
  execFileSync("npm", ["run", "build"], { stdio: "inherit" });
}

let manifest;
try {
  manifest = JSON.parse(readFileSync(MANIFEST, "utf8"));
} catch {
  console.error(
    `bundle-report: ${MANIFEST} not found. Run "npm run build" first, or pass --build.`,
  );
  process.exit(1);
}

const entry = Object.values(manifest).find((c) => c.isEntry);
if (!entry) {
  console.error("bundle-report: the manifest has no entry chunk.");
  process.exit(1);
}

const kb = (file) => statSync(join(DIST, file)).size / 1024;

/** The chunks the browser must fetch before the shell can render. */
const eagerFiles = new Set([entry.file]);
const walk = (key) => {
  const chunk = manifest[key];
  if (!chunk) return;
  for (const imported of chunk.imports ?? []) {
    if (eagerFiles.has(manifest[imported]?.file)) continue;
    if (manifest[imported]) eagerFiles.add(manifest[imported].file);
    walk(imported);
  }
};
walk(Object.keys(manifest).find((k) => manifest[k].isEntry));

const entryKb = kb(entry.file);
const eagerTotalKb = [...eagerFiles].reduce((sum, f) => sum + kb(f), 0);

// ---------- Report ----------

const all = Object.values(manifest)
  .filter((c) => c.file.endsWith(".js"))
  .map((c) => ({ file: c.file, kb: kb(c.file), eager: eagerFiles.has(c.file) }))
  .sort((a, b) => b.kb - a.kb);

console.log("\nChunks (eager = fetched before the shell renders):\n");
for (const c of all) {
  const source = readFileSync(join(DIST, c.file), "utf8");
  const hits = WATCHED.filter(([, re]) => re.test(source)).map(([name]) => name);
  console.log(
    `  ${c.eager ? "eager" : "lazy "}  ${c.kb.toFixed(1).padStart(8)} kB  ${c.file}` +
      (hits.length ? `   [${hits.join(", ")}]` : ""),
  );
}

console.log(
  `\n  entry chunk : ${entryKb.toFixed(1)} kB  (budget ${EAGER_BUDGET_KB} kB)`,
);
console.log(`  eager total : ${eagerTotalKb.toFixed(1)} kB\n`);

// ---------- Gate ----------

const problems = [];
if (entryKb > EAGER_BUDGET_KB) {
  problems.push(
    `the entry chunk is ${entryKb.toFixed(1)} kB, over the ${EAGER_BUDGET_KB} kB budget`,
  );
}

// The three heavyweights the split exists for must not come back.
const eagerSource = [...eagerFiles]
  .map((f) => readFileSync(join(DIST, f), "utf8"))
  .join("\n");
for (const name of ["xterm", "highlight.js", "marked"]) {
  const [, re] = WATCHED.find(([n]) => n === name);
  if (re.test(eagerSource)) {
    problems.push(`"${name}" is back in the eager graph; it must stay lazy`);
  }
}

if (problems.length) {
  console.error("bundle-report: budget exceeded\n");
  for (const p of problems) console.error(`  - ${p}`);
  console.error(
    "\nEither move the feature behind a lazy boundary (src/features/lazyFeatures.ts)\n" +
      "or raise EAGER_BUDGET_KB in vite.config.ts deliberately and say why in\n" +
      "Concept.md, §4 \"Frontend loading boundaries\".\n",
  );
  process.exit(1);
}

console.log("bundle-report: within budget.\n");
