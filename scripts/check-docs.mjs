#!/usr/bin/env node
// ============================================================
// Documentation that stays true (TICKET-037).
//
// Docs rot silently: a renamed file leaves a dead link, a removed
// script leaves an instruction that fails on a clean checkout, and a
// capability that moves leaves a matrix that quietly lies. None of that
// shows up in a build.
//
// So four things are checked, each cheap enough to run on every commit:
//
//   1. every repository-relative link and backticked path exists;
//   2. every `npm run …` a document tells a reader to run is a real
//      script in package.json;
//   3. the provider matrix in Concept.md matches the capability
//      contract in `vfs/caps.rs` — the executable source of truth;
//   4. every ticket a document references exists, open or done.
//
//   node scripts/check-docs.mjs
// ============================================================

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, normalize } from "node:path";

const problems = [];
const note = (file, message) => problems.push(`${file}\n    ${message}`);

// ---------- Collect the documents ----------

function markdownFiles(dir = ".", out = []) {
  for (const name of readdirSync(dir)) {
    // Vendored upstream docs retain their original links; our provenance is in docs/.
    if (["node_modules", "target", "dist", ".git", "vendor"].includes(name)) continue;
    const p = dir === "." ? name : join(dir, name);
    if (statSync(p).isDirectory()) markdownFiles(p, out);
    else if (p.endsWith(".md")) out.push(p);
  }
  return out;
}

const docs = markdownFiles();

/**
 * Everything below this heading in Concept.md is a chronological record
 * (TICKET-037). It names files as they were, including ones since
 * removed, so the path check stops here.
 */
const HISTORY_HEADING = "## 12. Implementation history";

// ---------- 1. Links and referenced paths ----------

/** A link target that is a repository path rather than a URL or anchor. */
function isRepoPath(target) {
  if (!target || target.startsWith("#")) return false;
  if (/^[a-z][a-z0-9+.-]*:/i.test(target)) return false; // http:, mailto:, …
  return true;
}

/**
 * The roots a backticked path may be written against.
 *
 * Documents name source files the way the code does — `lib/path.ts`,
 * `commands/fs/file.rs` — rather than repeating `src/` or
 * `src-tauri/src/` every time. Spelling them out would make the prose
 * worse, so the check resolves against the roots instead.
 */
const ROOTS = ["", "src/", "src/features/", "src-tauri/src/", "src-tauri/"];

function resolveAgainstRoots(target) {
  return ROOTS.some((root) => existsSync(root + target));
}

/**
 * A completed ticket moves from `tickets/NNN-x.md` to
 * `tickets/done/NNN-x.done.md`. Links written before the move are not
 * wrong — the ticket still exists — so the check follows it.
 */
function resolveTicketLink(target) {
  const m = /^(?:.*\/)?(\d{3}-[a-z0-9-]+)\.md$/.exec(target);
  if (!m) return false;
  return (
    existsSync(`tickets/${m[1]}.md`) ||
    existsSync(`tickets/done/${m[1]}.done.md`)
  );
}

for (const doc of docs) {
  const text = readFileSync(doc, "utf8");
  const dir = doc.includes("/") ? doc.slice(0, doc.lastIndexOf("/")) : ".";

  for (const m of text.matchAll(/\[[^\]]*\]\(([^)\s]+)\)/g)) {
    const target = m[1].split("#")[0];
    if (!isRepoPath(target)) continue;
    const resolved = target.startsWith("/")
      ? target.slice(1)
      : dir === "."
        ? target
        : join(dir, target);
    if (existsSync(normalize(resolved))) continue;
    if (resolveTicketLink(target)) continue; // a completed ticket moved
    note(doc, `dead link: ${m[1]}`);
  }

  // Paths named in backticks are checked only in the *living*
  // documentation. A ticket is either a plan (naming files it will
  // create) or a record (naming files as they were); gating the build
  // on either would mean editing history to keep a build green.
  if (doc.startsWith("tickets/")) continue;
  // The history section names files that were deliberately removed.
  // Checking those would mean keeping deleted files around to satisfy a
  // record of their deletion.
  const living = text.split(HISTORY_HEADING)[0];
  for (const m of living.matchAll(/`([^`\s]+\.(?:rs|ts|tsx|json|yml|toml|mjs|md))`/g)) {
    const target = m[1].replace(/^\.\//, "");
    // `NNN` is how the documents write a placeholder for a number.
    if (target.includes("*") || target.startsWith("@/") || target.includes("NNN"))
      continue;
    // A bare file name is often shorthand ("see mod.rs"); only complain
    // about something that looks like a full path.
    if (!target.includes("/")) continue;
    if (resolveAgainstRoots(target)) continue;
    if (resolveTicketLink(target)) continue;
    note(doc, `referenced path does not exist: ${m[1]}`);
  }
}

// ---------- 2. Documented npm scripts ----------

const scripts = new Set(
  Object.keys(JSON.parse(readFileSync("package.json", "utf8")).scripts ?? {}),
);

for (const doc of docs) {
  const text = readFileSync(doc, "utf8");
  for (const m of text.matchAll(/npm run ([a-z][a-z0-9:-]*)/g)) {
    if (!scripts.has(m[1])) {
      note(doc, `documents "npm run ${m[1]}", which package.json does not define`);
    }
  }
}

// ---------- 3. The provider matrix against the capability contract ----------

const caps = readFileSync("src-tauri/src/vfs/caps.rs", "utf8");

/** The capability ids the backend declares. */
const declared = new Set(
  [...caps.matchAll(/Capability::(\w+) => "(\w+)"/g)].map((m) => m[2]),
);

/** What `remote_core_caps()` lists — the row that must read ✅ everywhere. */
const remoteCore = (() => {
  const at = caps.indexOf("fn remote_core_caps()");
  if (at < 0) return null;
  const body = caps.slice(at, caps.indexOf("\n}", at));
  return new Set(
    [...body.matchAll(/^\s{8}(\w+),$/gm)].map((m) =>
      m[1].replace(/^./, (c) => c.toLowerCase()),
    ),
  );
})();

const concept = readFileSync("Concept.md", "utf8");
const matrixAt = concept.indexOf("| Capability | local |");
if (matrixAt < 0) {
  problems.push(
    "Concept.md\n    the provider matrix is gone — it is the documented form of vfs/caps.rs",
  );
} else {
  const matrix = concept.slice(matrixAt, concept.indexOf("\n\n", matrixAt));
  const rows = matrix.split("\n").slice(2);
  const documented = new Set();
  for (const row of rows) {
    const cells = row.split("|").map((c) => c.trim());
    const names = [...cells[1].matchAll(/`([^`]+)`/g)].map((m) => m[1]);
    // "archiveExtractFrom/To" is one cell for two capabilities.
    for (const name of names) {
      const slash = name.indexOf("/");
      if (slash > 0) {
        const stem = name.slice(0, slash).replace(/(From|To)$/, "");
        for (const suffix of name.slice(slash + 1).split("/")) {
          documented.add(stem + suffix);
        }
        documented.add(name.slice(0, slash));
      } else {
        documented.add(name);
      }
    }
    // The remote-core row must show ✅ for all four protocols.
    if (remoteCore && names.some((n) => remoteCore.has(n))) {
      const remoteCells = cells.slice(3, 7);
      if (remoteCells.some((c) => c !== "✅")) {
        note(
          "Concept.md",
          `"${cells[1]}" is in remote_core_caps() but the matrix does not show it for every protocol`,
        );
      }
    }
  }

  for (const cap of declared) {
    if (!documented.has(cap)) {
      note("Concept.md", `capability "${cap}" is declared in vfs/caps.rs but missing from the matrix`);
    }
  }
  for (const cap of documented) {
    if (!declared.has(cap)) {
      note("Concept.md", `matrix lists "${cap}", which vfs/caps.rs does not declare`);
    }
  }
}

// ---------- 4. Referenced tickets ----------

const tickets = new Set();
for (const dir of ["tickets", "tickets/done"]) {
  if (!existsSync(dir)) continue;
  for (const name of readdirSync(dir)) {
    const m = /^(\d{3})-/.exec(name);
    if (m) tickets.add(m[1]);
  }
}

for (const doc of docs) {
  if (doc.startsWith("tickets/")) continue; // a ticket may plan ahead
  const text = readFileSync(doc, "utf8");
  for (const m of text.matchAll(/TICKET-(\d{3})/g)) {
    if (!tickets.has(m[1])) {
      note(doc, `references TICKET-${m[1]}, which has no file in tickets/`);
    }
  }
}

// ---------- Report ----------

if (problems.length) {
  console.error("check-docs: documentation is out of step\n");
  for (const p of problems) console.error(`  - ${p}\n`);
  process.exit(1);
}
console.log(
  `check-docs: ${docs.length} documents — links, scripts, the provider matrix and ticket references all check out.`,
);
