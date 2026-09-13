# Transfer preflight

Copy and move confirmation includes a read-only plan: selected sources, destination,
file count, additional space, peak footprint, capacity/quota, safety mode,
verification reads and warnings. Blocking issues disable confirmation. Closing the
dialog cancels traversal. The approved settings and plan digest accompany the
queued request; changes to settings do not alter waiting work.

## Planning and revalidation

The Rust `OperationPlan` contains normalized provider locations, endpoint/profile
identities, connection generations, per-source metadata summaries, destination
identity, collision state, settings and estimates. Remote locations use profile
IDs and endpoint digests rather than credential-bearing URLs. Issue records use
stable reason codes and source indices; diagnostics do not need raw paths.

`transfer_preflight` only reads. It does not create journal records, temporary
files or execution authority. The digest is a consistency check, not a capability:
queuing still supplies the complete authorized operation and checks live provider
capabilities. The queue recomputes the plan and rejects changed reviewed semantics
before saving it in the existing private recovery journal. The engine recomputes
again before work starts. Every source is revalidated before writes; destination
identity, connection generation, writable/staging state and capacity are checked
before writes and immediately before commit. Staged publication also rejects a
changed target. An overwrite decision remains a fresh transfer collision decision.

Connection generations distinguish provider instances even for identical store
names. Weak references prevent allocator address reuse from reviving an old
instance. Reconnect, edited credentials/trust or profile deletion cannot reuse a
previous provider generation. A recovered transfer is newly validated; no dry-run
result authorizes reconnect or bypasses trust verification.

## Constraints and space

Provider limits are optional: available bytes, remaining quota, maximum object,
name/path limits, writable state, case sensitivity, staging, reserved names and
metadata fidelity. If capacity and quota are both known, the smaller value applies.
Unknown capacity is explicitly unknown, not unlimited. macOS local planning queries
write access and POSIX name/path limits. Other native/provider constraints remain
unknown unless their adapter supplies them. Changes at the actual write can still
cause errors after a successful preview.

Plans reject recursive/self-copy, repeated/nested sources, ambiguous destination
names, known invalid names/object limits, missing/unreadable entries and known
capacity/staging failures. When case sensitivity is unknown, normalized lowercase
name collisions are conservatively rejected. This is not a promise to emulate
every remote filesystem's Unicode name-equivalence rules. Provider-unknown rules
and metadata fidelity are visible advisories; unsupported write results still fail
through the normal transfer error handling.

Space distinguishes **additional allocation** from **peak footprint**. Staging a
100-byte replacement of a 40-byte existing file needs 100 additional bytes and has
a 140-byte attributable peak. Renaming the existing file into a backup does not
allocate another 40 bytes. Verification reads source and output; it does not require
another full output copy. Same-volume native moves allocate no second data copy;
cross-volume moves budget destination data. Filesystem metadata allocation, sparse
extent preservation and compression are not guaranteed savings.

Archive extraction checks selected declared output size before creating its staging
directory; packing uses a conservative incompressible ZIP bound including entry
and compression overhead. Existing archive validation remains authoritative for
actual expanded bytes. Backup files already occupy capacity and are not charged
as newly allocated bytes. Space is rechecked by the ordinary write/commit paths;
no estimate reserves disk blocks.

## Bounds and validation

A plan retains top-level mappings and directory summaries, not a duplicate full
file manifest. Limits: 1,000 sources, one million visited nodes including collision
inspection, depth 128, 50,000 retained sibling names, 16 MiB of name strings and
32 issues. Local directories stream from the OS. Protocols whose current APIs
return a directory vector still materialize that provider response; the planner
rejects oversized listings and does not retain a second full tree. Exceeding a
bound requires smaller selections. Every visit observes cancellation.

Tests cover overlap/nesting/duplicate mappings, checked arithmetic, known/unknown
and contradictory capacity/quota, new generations, changed sources/replaced
folders, missing entries, symlink loops, provider name/object limits, synthetic
million-entry traversal bounds and cancellation. UI tests cover DE/EN blocked
confirmation, cancellation and approval of the exact settings snapshot. Existing
archive and transfer fault tests continue checking publication safety.
