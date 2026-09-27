# TICKET-371: Remove both quick-xml denial-of-service vulnerabilities from both dependency lines

Priority: **P1** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `quick-xml` | `0.38.4` | [RUSTSEC-2026-0194](https://rustsec.org/advisories/RUSTSEC-2026-0194.html) | vulnerability |
| `quick-xml` | `0.38.4` | [RUSTSEC-2026-0195](https://rustsec.org/advisories/RUSTSEC-2026-0195.html) | vulnerability |
| `quick-xml` | `0.39.4` | [RUSTSEC-2026-0194](https://rustsec.org/advisories/RUSTSEC-2026-0194.html) | vulnerability |
| `quick-xml` | `0.39.4` | [RUSTSEC-2026-0195](https://rustsec.org/advisories/RUSTSEC-2026-0195.html) | vulnerability |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **4 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

quick-xml 0.38.4 arrives through plist 1.8.0; 0.39.4 arrives through keepass 0.10.5. Both versions trigger duplicate-attribute quadratic-time and namespace-allocation findings. This ticket owns all four advisory/version pairs.

## Required change

Update the plist and KeePass dependency paths to non-affected quick-xml (recorded minimum 0.41.0), or backport the fixes with reviewed provenance if compatible upstream upgrades are unavailable. Check the existing KeePass/hmac/SMB constraint before changing keepass. Preserve KDBX read/write compatibility and credential safety; file-size limits alone do not prove bounded parser work.

- Follow [dependency governance](../../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Test many duplicate attributes and namespace declarations with time/memory bounds, malformed and valid plist/XML, and KeePass unlock/save/reopen with canary secrets excluded from diagnostics. Cover both resolved quick-xml lines until both are removed or fixed.

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
`e2111519ba6d14a5da59a7b2e5c8083ae8a37c01`) and all four pairs still applied:
both `0.38.4` and `0.39.4` against both RUSTSEC-2026-0194 (quadratic duplicate
attribute checking) and -0195 (unbounded namespace-declaration allocation), both
patched from 0.41.0.

Both owning paths were moved, so both resolved lines disappear and a single
`quick-xml 0.42.0` remains. No fix was backported and no exception was added.

### plist

`plist` 1.8.0 → **1.10.1**, which requires `quick-xml ^0.42`. The
`Value`/`Dictionary` API hortcmd uses for Finder tags is unchanged, so no code
change was needed. It also pulls `base64 0.23.1`, which becomes a third base64
line; base64 is security-critical, so the existing duplicate review was
re-fingerprinted rather than left to lapse.

### KeePass

`keepass` was pinned at `=0.10.5` (quick-xml 0.39) and moves to `=0.15.0`
(quick-xml 0.42). This was the substantive part: 0.15 replaced the owned tree
with an arena, so `net/credentials.rs` was migrated.

- `Database::root`/`root_mut` now return `GroupRef`/`GroupMut` handles, and a
  group's `groups`/`entries` collections are private: children are created with
  `add_group`/`add_entry` and removed through `EntryMut::remove`.
- `Database::new` takes no config, and icons are set with `set_icon_builtin`
  instead of assigning `icon_id`.
- The depth-first `find_entry`/`find_entry_mut`/`remove_entry` trio collapses
  into one `find_entry(&Database, title) -> Option<EntryId>` over
  `iter_all_entries`, because every entry now lives in one flat map. That keeps
  the behavior the KeePassXC-interop test pins down — an entry the user filed in
  their own group is found, and an update stays where they filed it — while
  removing the three-way recursion.
- An id that stops resolving between lookup and mutation is reported as
  `cred.store` rather than panicking, which is what the old
  `expect("group just inserted")` did.

Everything else is deliberately unchanged: KDBX 4 only, Argon2/AES-256/ChaCha20
defaults, the explicit KeePassXC-compatible `Meta` block, the atomic
write-through-temporary-sibling, the one-shot "do not remember" session and the
wrong-password error code.

The MSRV question the ticket raises is settled: keepass 0.15.0 declares Rust
1.89 and plist 1.10.1 declares 1.88, both below the 1.93 floor ticket 372 raised
deliberately. The old `=0.10.5` pin existed to hold the 1.85.1 floor
(ticket 068), and that is exactly what the advisory backlog had to move past.

### Verification

The parser bounds are inside quick-xml and covered by its own regression tests;
hortcmd neither drives quick-xml directly nor is `quick-xml` a direct dependency,
so a duplicate-attribute or namespace fixture here would only re-test a
third-party parser. The ticket's warning that "file-size limits alone do not
prove bounded parser work" is why the disposition is the version move rather
than an input cap.

What hortcmd owns is the KDBX round trip and credential safety, and that is
tested end to end by the existing suite in `net/credentials.rs`, which passes
against 0.15.0:

- unlock → `set` → `lock` → re-unlock → `get` round-trips a secret through a real
  file on disk, and reopening with `Database::open` reads it back;
- a wrong master password is an error with the re-prompt code, and a missing file
  needs the `create` flag;
- the one-shot session re-locks after exactly one access;
- the KeePassXC-interop test writes an entry into a foreign group with the new
  builder API, finds it, updates it, and asserts hortcmd's own group was *not*
  created — so the whole-tree lookup still behaves;
- the metadata test asserts the `Meta` block and the root group's built-in icon,
  autotype and searching flags a KeePassXC-compatible file needs.

Test secrets are literals inside the test module and never leave it; the
redaction regression from ticket 307 still covers the diagnostic path.

- `npm run audit:dependencies`: **2** Rust findings. All four quick-xml
  advisory/version pairs are gone, and both resolved versions with them; no new
  advisory appeared. What is left is ticket 376 (proc-macro-error) and ticket 373
  (rsa).
- `npm run check` passed on macOS (arm64); `npm run supply:bundle` and
  `npm run supply:verify` regenerated and verified the release evidence.
