#!/usr/bin/env node
// ============================================================
// Architecture boundaries (TICKET-035).
//
// Splitting a large module only pays off if the pieces keep pointing
// the same way. Two things quietly undo it: a cycle (A imports B
// imports A), which makes "read one without the other" impossible
// again, and an upward import that reverses a layer.
//
// So both are checked rather than agreed. The rules below are the
// dependency directions the refactor established; each says why.
//
//   node scripts/check-boundaries.mjs
// ============================================================

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname, resolve, relative } from "node:path";

const ROOT = "src";
const RUST = "src-tauri/src";

// ---------- Collect the TypeScript import graph ----------

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.tsx?$/.test(p) && !/\.test\.tsx?$/.test(p)) out.push(p);
  }
  return out;
}

const files = walk(ROOT);
/** file → the project files it imports */
const graph = new Map();

for (const file of files) {
  const source = readFileSync(file, "utf8");
  const targets = new Set();
  // Type-only imports are erased at build time: they cannot form a
  // runtime cycle and they do not make one module depend on another's
  // behaviour. Only value imports count here.
  const specs = [
    ...source.matchAll(/^import\s+(?!type\s)[^'"]*from\s+"([^"]+)"/gm),
    ...source.matchAll(/\bimport\(\s*"([^"]+)"\s*\)/g),
    ...source.matchAll(/^export\s+(?!type\s)[^'"]*from\s+"([^"]+)"/gm),
  ]
    .map((m) => m[1])
    // `import { type A, value } from "x"` still imports a value.
    .filter(Boolean);

  for (const spec of specs) {
    let base;
    if (spec.startsWith("@/")) base = join(ROOT, spec.slice(2));
    else if (spec.startsWith(".")) base = resolve(dirname(file), spec);
    else continue; // a package
    const rel = relative(process.cwd(), base);
    for (const cand of [
      rel,
      `${rel}.ts`,
      `${rel}.tsx`,
      join(rel, "index.ts"),
      join(rel, "index.tsx"),
    ]) {
      if (files.includes(cand)) {
        targets.add(cand);
        break;
      }
    }
  }
  graph.set(file, [...targets]);
}

// ---------- Cycles ----------

const problems = [];
const state = new Map(); // file → "visiting" | "done"
const stack = [];

function visit(file) {
  const seen = state.get(file);
  if (seen === "done") return;
  if (seen === "visiting") {
    const cycle = [...stack.slice(stack.indexOf(file)), file];
    problems.push(`import cycle: ${cycle.join("\n                → ")}`);
    return;
  }
  state.set(file, "visiting");
  stack.push(file);
  for (const next of graph.get(file) ?? []) visit(next);
  stack.pop();
  state.set(file, "done");
}
for (const file of files) visit(file);

// ---------- Layer rules ----------

/**
 * Each rule: files matching `from` may not import anything matching
 * `to`. The `why` is what a failure prints — a rule nobody can explain
 * is a rule nobody will keep.
 */
const RULES = [
  {
    from: /^src\/lib\//,
    to: /^src\/(store|features|components)\//,
    why: "lib/ is leaf utilities: pure, reusable, and unaware of the app's state or UI.",
  },
  {
    from: /^src\/ipc\/(invoke|events|demo|types)\.ts$/,
    to: /^src\/ipc\/domains\//,
    why: "The IPC foundation is below the domain clients; only they may reach down.",
  },
  {
    from: /^src\/ipc\//,
    to: /^src\/(store|features|components)\//,
    why: "The IPC layer is a transport. Knowing about stores or UI would make it one more place the app's shape leaks into.",
  },
  {
    from: /^src\/i18n\//,
    to: /^src\/(store|features|components|ipc)\//,
    why: "Translations are data. Nothing about the app may reach into them.",
  },
  {
    from: /^src\/store\/panes\/state\.ts$/,
    to: /^src\/store\/panesStore\.ts$/,
    why: "The pure pane transitions must not depend on the store that uses them — that is the whole point of extracting them.",
  },
];

for (const [file, targets] of graph) {
  for (const rule of RULES) {
    if (!rule.from.test(file)) continue;
    for (const target of targets) {
      if (rule.to.test(target)) {
        problems.push(`${file}\n    imports ${target}\n    ${rule.why}`);
      }
    }
  }
}

// ---------- Rust module direction ----------
//
// The transfer engine's layers are stated in its own mod.rs; a module
// importing one above it would undo that ordering.
const RUST_LAYERS = ["events", "registry", "progress", "plan", "copy", "parallel", "engine"];
const RUST_ALLOWED = {
  // `events` is the bottom layer — the serialized payloads depend on
  // nothing and everything above may use them.
  events: [],
  registry: ["events"],
  progress: ["events"],
  plan: ["events"],
  copy: ["events", "plan", "progress"],
  parallel: ["copy", "events", "plan", "progress"],
  engine: ["copy", "events", "parallel", "plan", "progress", "registry"],
};
for (const mod of RUST_LAYERS) {
  const path = join(RUST, "commands/fs/transfer", `${mod}.rs`);
  let source;
  try {
    source = readFileSync(path, "utf8");
  } catch {
    problems.push(`missing transfer module: ${path}`);
    continue;
  }
  for (const m of source.matchAll(/use super::(\w+)/g)) {
    const target = m[1];
    if (!RUST_LAYERS.includes(target)) continue;
    if (!RUST_ALLOWED[mod].includes(target)) {
      problems.push(
        `${path}\n    uses super::${target}\n    ` +
          `The transfer layers run downwards (see transfer/mod.rs); ` +
          `"${mod}" may only use: ${RUST_ALLOWED[mod].join(", ") || "nothing"}.`,
      );
    }
  }
}

// ---------- Report ----------

if (problems.length) {
  console.error("check-boundaries: architecture rules violated\n");
  for (const p of problems) console.error(`  - ${p}\n`);
  process.exit(1);
}
console.log(
  `check-boundaries: ${files.length} modules, no cycles, all layer rules hold.`,
);
