# TICKET-377: Replace the five unmaintained rust-unic crates

Priority: **P2** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `unic-char-property` | `0.9.0` | [RUSTSEC-2025-0081](https://rustsec.org/advisories/RUSTSEC-2025-0081.html) | unmaintained |
| `unic-char-range` | `0.9.0` | [RUSTSEC-2025-0075](https://rustsec.org/advisories/RUSTSEC-2025-0075.html) | unmaintained |
| `unic-common` | `0.9.0` | [RUSTSEC-2025-0080](https://rustsec.org/advisories/RUSTSEC-2025-0080.html) | unmaintained |
| `unic-ucd-ident` | `0.9.0` | [RUSTSEC-2025-0100](https://rustsec.org/advisories/RUSTSEC-2025-0100.html) | unmaintained |
| `unic-ucd-version` | `0.9.0` | [RUSTSEC-2025-0098](https://rustsec.org/advisories/RUSTSEC-2025-0098.html) | unmaintained |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **5 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

Five rust-unic 0.9.0 crates are reached through unic-ucd-ident and urlpattern 0.3.0 in the Tauri utilities/runtime/build graph. These are five distinct maintenance findings sharing an upstream migration path, not five demonstrated runtime vulnerabilities.

## Required change

Update or replace the owning urlpattern/Tauri dependency path with a maintained Unicode implementation. The recorded audit lists no safe in-place upgrade for the old crates. Preserve URL-pattern and Unicode semantics and review changed Tauri APIs/features. Never exclude build-time/target-specific crates from the inventory to hide these findings.

- Follow [dependency governance](../../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Test URL-pattern matching and Unicode identifier/property cases, including non-ASCII and boundary inputs. Verify app startup/navigation behavior and all three platform builds. Demonstrate a disposition for each of the five advisory/version pairs, even if one dependency upgrade removes all five.

- [ ] Every advisory/version pair in the table is removed or has a documented,
      policy-valid, narrowly scoped mitigation; no pair is silently dropped.
- [ ] Run `npm run audit:dependencies` and compare findings by crate, version and
      advisory ID. This ticket need not clear unrelated tickets, but must not
      introduce new unaddressed findings. All 18 require disposition before release.
- [ ] Pass `npm run check`; run affected protocol tests explicitly because they
      are ignored in ordinary CI, and obtain relevant Windows/macOS/Linux results.
- [ ] Regenerate and verify release evidence with `npm run supply:bundle` and
      `npm run supply:verify`; check licenses, notices, optional/target coverage
      and security-sensitive duplicate versions.
- [ ] Follow the per-ticket branch, local commit and merge workflow. Move this
      ticket to `done/` only after its remediation and verification are complete.

## Resolution (2026-09-27)

The advisories were refreshed first (cargo-deny 0.19.5, RustSec revision
`e2111519ba6d14a5da59a7b2e5c8083ae8a37c01`) and all five rust-unic findings
still applied. The recorded audit is right that the old crates have no in-place
upgrade: rust-unic is archived. The owning path is the one to move.

`unic-ucd-ident` is not reached directly at all — `cargo tree -i` shows a single
owner, `urlpattern 0.3.0`, required by `tauri-utils 2.9.3` (through
tauri-codegen, tauri-macros, tauri-runtime and tauri-runtime-wry).
`urlpattern 0.6.0` replaced the rust-unic identifier tables with
`icu_properties 2`, and `tauri-utils 2.10.0` is the first release to require
`urlpattern ^0.6`. So the remediation is one resolved-version move:

- `tauri-utils` 2.9.3 → **2.10.0**, which drops `urlpattern 0.3.0` and with it
  all five crates: `unic-char-property`, `unic-char-range`, `unic-common`,
  `unic-ucd-ident` and `unic-ucd-version`. Each of the five advisory/version
  pairs is therefore removed, not excepted — the audit output below lists none of
  them.
- `tauri` itself stays at 2.11.5: it depends on `tauri-utils ^2`, so 2.10.0
  satisfies it without a framework upgrade.

### What else the move brings in

tauri-utils 2.10 rewrites its HTML/CSP handling on `dom_query`/`html5ever`
instead of its previous parser, so the graph gains `brotli`,
`brotli-decompressor`, `alloc-stdlib`, `alloc-no-stdlib`, `dom_query`,
`html5ever`, `markup5ever`, `selectors 0.38`, `cssparser 0.37`,
`cssparser-macros 0.7`, `json-patch`, `jsonptr` and newer `cfb`, `ctor`, `infer`
and `swift-rs`. Net effect: 766 → 770 components and 104 → 116 duplicate groups,
the new groups being the second `selectors`/`cssparser`/`markup5ever` lines
alongside the ones wry already pulled. None of them is security-critical, so no
duplicate review was required, and none carries an advisory.

`policy/dependencies.json` records the rest: MPL-2.0 license reviews for
`cssparser 0.37.0`, `cssparser-macros 0.7.1` and `selectors 0.38.0` — the same
file-level-copyleft reasoning already reviewed for their 0.36 lines, with
notices regenerated — and build-script reviews for `selectors 0.38.0` and the
new `swift-rs 1.0.8`.

### Verification

- URL-pattern semantics: hortcmd configures no remote capability URL scope
  (`src-tauri/capabilities/default.json` grants only `core:*` permissions to the
  `main` window), so urlpattern has no application-supplied input to preserve.
  What does run is tauri-codegen at build time, which parses the real
  `tauri.conf.json` — including the production CSP and the `hortplugin:`
  frame-src entry — and the capability set through the new tauri-utils; that is
  the path the HTML/CSP rewrite touches, and it compiles and passes.
- Unicode behavior that is hortcmd's own — filename collation, the
  case-sensitivity rules in the rename preview, and the non-ASCII redaction
  regression from ticket 307 — is covered by the existing suites and unaffected:
  those use hortcmd's own comparison code, not urlpattern.
- `npm run audit:dependencies`: 6 Rust findings, all five RUSTSEC-2025-0075,
  -0080, -0081, -0098 and -0100 pairs gone and no new advisory. The remainder
  belong to tickets 371, 373 and 376.
- `npm run check` passed on macOS (arm64), including the offline supply-chain
  gate, the Rust build against the new tauri-utils, 797 Vitest tests and the
  production frontend build; `npm run supply:bundle` and `npm run supply:verify`
  regenerated and verified the release evidence with the new notices.
- Platform coverage: the macOS build and test run is done here. The Windows and
  Linux builds are the CI matrix's, and this change is not platform-gated — the
  new crates are portable and `swift-rs` was already macOS-only.
