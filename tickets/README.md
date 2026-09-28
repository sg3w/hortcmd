# Ticket organization

All tickets below 100 and tickets 100–110 have been completed.
Tickets 125–126, 128, 131, 134, 137 and the discovered issues 300–301 and 303–316 are also complete. There are
**29 open tickets**: 111–124, 127, 129–130, 132–133, 135–136 and the discovered
issues 317–387 (excluding completed tickets listed below). Dependencies use the current paths.

Completed tickets retain their original numbers and live in `tickets/done/`, with
filenames ending in `.done.md`. This also applies to previously completed support
tickets with higher numbers. Do not reuse those numbers for new work.

- 100–108: plugin contract, hosts, integration and the first storage-usage plugin.
- 109–137: the remaining support, safety, maintenance and verification backlog.
- Native Windows/Linux verification remains in its existing relative position.

## Renumbering on 2026-09-15

| Previous number | Current ticket | Scope |
| --- | --- | --- |
| 201 | [100](done/100-plugin-api-contract.done.md) | Plugin API & manifest (contract first) |
| 202 | [101](done/101-js-plugin-host.done.md) | JS plugin host (sandboxed WebviewWindow) |
| 203 | [102](done/102-native-plugin-host.done.md) | Native Rust plugin host (dynamic library) |
| 204 | [103](done/103-plugin-commandbar.done.md) | Plugin command bar & colorful icons |
| 205 | [104](done/104-plugin-context-handoff.done.md) | Context handoff to plugins |
| 206 | [105](done/105-plugin-actions-events.done.md) | Mediated actions & events |
| 207 | [106](done/106-plugin-management.done.md) | Plugin management (settings) |
| 208 | [107](done/107-reference-demo-plugin.done.md) | Reference/demo plugin(s) |
| 209 | [108](done/108-folder-storage-usage-visualizer.done.md) | First functional plugin — folder storage-usage visualizer |
| 220 | [109](done/109-redacted-diagnostics-and-support-bundle.done.md) | Add redacted diagnostics and an exportable support bundle |
| 221 | [110](done/110-dependency-and-license-governance.done.md) | Establish dependency, vulnerability, and license governance |
| 222 | [111](111-signed-reproducible-releases-and-updates.md) | Produce signed, reproducible releases with safe updates |
| 223 | [112](112-performance-benchmarks-and-budgets.md) | Establish performance benchmarks and regression budgets |
| 225 | [113](113-safe-archive-extraction.md) | Contain archive extraction and limit hostile input |
| 249 | [114](114-accessibility-and-localization-quality.md) | Establish accessibility and localization quality gates |
| 250 | [115](115-windows-linux-screen-reader-verification.md) | Complete native Windows/NVDA and Linux/Orca verification |
| 251 | [116](116-windows-linux-transfer-recovery.md) | Enable and verify transfer recovery on Windows and Linux |
| 252 | [117](117-native-transport-security-verification.md) | Verify transport security on native platforms and SMB servers |
| 253 | [118](118-negotiated-tls-diagnostics.md) | Expose negotiated TLS details from client APIs |
| 254 | [119](119-file-undo-windows-linux-adapters.md) | Validate file undo on Windows and Linux |
| 255 | [120](120-preflight-native-provider-limits.md) | Validate native Windows/Linux preflight constraints |
| 256 | [121](121-search-index-windows-linux-adapters.md) | Native Windows/Linux scope validation and resource controls for indexed search |
| 257 | [122](122-remote-index-stable-content-capabilities.md) | Prove remote no-follow and stable-content reads for optional indexing |
| 259 | [123](123-vfs-native-provider-path-conformance.md) | Verify native Windows/Linux VFS provider paths |
| 260 | [124](124-removable-media-native-verification.md) | Removable-media hardware and platform verification |
| 261 | [125](done/125-index-search-session-restore.done.md) | Restore the index search dialog within the session |
| 262 | [126](done/126-archive-navigation-history.done.md) | Retain archive places in navigation history |
| 263 | [127](127-native-paired-comparison-verification.md) | Native Prüfung der doppelten Vergleichsliste |
| 264 | [128](done/128-rename-dialog-structured-errors.done.md) | Strukturierte Batch-Fehler im Umbenennen-Dialog anzeigen |
| 265 | [129](129-windows-linux-native-file-icons.md) | Native Dateisymbole unter Windows und Linux |
| 266 | [130](130-native-icon-gui-verification.md) | Native GUI-Prüfung der Dateisymbole ergänzen |
| 267 | [131](done/131-zip-lzma-framing-conformance.done.md) | Correct ZIP-LZMA framing with independent fixtures |
| 268 | [132](132-archive-budget-upstream-migration.md) | Lokale Archiv-Budgetpatches upstream ersetzen |
| 270 | [133](133-atomic-provider-staging-ownership.md) | Reserve provider staging entries atomically and verify ownership |
| 273 | [134](done/134-jsdom-canvas-test-diagnostic.done.md) | Isolate the missing canvas diagnostic in frontend tests |
| 274 | [135](135-command-line-native-verification.md) | Native Windows/Linux command-line verification |
| 275 | [136](136-native-trash-error-verification.md) | Verify native trash errors and refused Finder automation |
| 276 | [137](done/137-lazy-feature-translations.done.md) | Load feature translations with their lazy UI |

## Earlier moves

Older history may describe the prior scheduling: 056 became 209 (now 108),
045 became 220 (now 109), 050 became 221 (now 110), 051 became 222 (now 111),
052 became 223 (now 112), 046 became 225 (now 113), and 043 became 249 (now 114).
074 became [224](done/224-vitest-module-mock-hoisting.done.md), which is already
completed and retains that number. The table above preserves the identity of
historical references while active links use the current filenames.

## Newly discovered issues

- [300](done/300-rename-request-lifecycle.done.md): batch-rename request errors, duplicate submissions and stale completions.

- [301](done/301-clipboard-write-failure-reporting.done.md): shared clipboard writes silently succeed after refusal.

- [303](done/303-sftp-incoming-packet-memory-bound.done.md): bound incoming SFTP packet allocation before reading the body.

- [304](done/304-sftp-file-type-discriminants.done.md): special SFTP files can be misclassified as directories.

- [305](done/305-archive-password-error-dispatch.done.md): preserve structured archive password errors for password prompts.

- [306](done/306-archive-listing-error-presentation.done.md): present structured archive listing failures correctly.

## Discovered in the 2026-09-22 full-codebase review

62 findings from a 16-way parallel code review of the entire codebase
(11 Rust backend areas, 5 frontend areas), covering correctness, security,
data-safety, efficiency, and reuse. Ordered roughly most-severe first;
numbers are otherwise unordered within that grouping.

Security/critical:
[307](done/307-error-redaction-byte-char-index-mismatch.done.md) (error redaction silently
fails on non-ASCII input — verified credential leak),
[308](done/308-mcp-error-detail-not-redacted.done.md), [309](done/309-apperror-fields-not-encapsulated.done.md),
[310](done/310-connections-dialog-secret-field-leak.done.md), [311](done/311-webdav-spool-file-symlink-race.done.md),
[312](done/312-mcp-extract-missing-live-authorization.done.md).

Data-safety (transfer/archive/undo/search):
[313](done/313-discard-staging-remote-destination-noop.done.md), [314](done/314-transfer-commit-rollback-failure-swallowed.done.md),
[315](done/315-parallel-copy-orphaned-directories.done.md), [316](done/316-zip-directory-entries-skip-manifest-verify.done.md),
[317](done/317-undo-eligibility-canonicalize-hard-error.done.md), [318](done/318-search-unreadable-files-not-counted.done.md).

Correctness (backend):
[319](done/319-smb-remove-dir-all-reparse-point-recursion.done.md), [320](done/320-ftp-webdav-missing-connection-timeout.done.md),
[321](done/321-vfs-listing-unbounded-memory-allocation.done.md), [322](322-native-plugin-windows-orphan-process.md),
[323](done/323-plugin-fs-preview-missing-capability-check.done.md), [324](done/324-checksums-missing-capability-check.done.md),
[325](done/325-selection-save-load-missing-capability-check.done.md), [326](done/326-compare-dirs-aborts-on-single-entry-error.done.md),
[327](done/327-filecompare-utf8-truncation-misclassification.done.md), [328](done/328-compare-dirs-stack-overflow-recursion.done.md),
[329](done/329-search-index-stale-global-cache-epoch.done.md), [330](done/330-search-index-crawler-step-not-cancellable.done.md),
[331](done/331-net-save-profile-monitor-stopped-before-validation.done.md), [332](done/332-probe-transport-missing-host-param.done.md),
[333](done/333-monitor-unsupported-state-latched.done.md), [334](done/334-keepass-unlock-state-divergence.done.md),
[335](done/335-cancel-token-map-orphan-leak.done.md), [339](done/339-smb-ensure-share-race-condition.done.md),
[340](done/340-webdav-list-dot-dotdot-not-filtered.done.md), [341](done/341-smb-shares-error-generic-code.done.md),
[342](done/342-mcp-mutex-lock-expect-poison-panic.done.md), [343](done/343-dir-size-blocking-call-not-spawn-blocking.done.md),
[344](done/344-finder-tags-dead-fallback-code.done.md), [345](done/345-git-status-canonicalize-fallback-scope-widening.done.md),
[346](done/346-dir-size-cancel-token-leak.done.md), [347](done/347-dir-size-generic-not-a-directory-error.done.md),
[349](done/349-pack-sources-count-not-bounded-early.done.md), [350](done/350-archive-unknown-format-generic-error.done.md),
[351](done/351-error-code-all-missing-tls-codes.done.md), [352](done/352-menu-language-apply-failure-silent.done.md),
[353](done/353-probe-transport-unbounded-total-timeout.done.md).

Correctness (frontend):
[336](done/336-fill-params-placeholder-resubstitution.done.md), [337](done/337-index-ipc-missing-hastauri-guard.done.md),
[338](done/338-use-tauri-hastauri-not-reactive.done.md), [348](done/348-progress-overlay-no-focus-trap.done.md),
[354](done/354-reactive-settings-subscription-bulk-mount.done.md), [355](done/355-normalize-associations-dead-code.done.md),
[356](done/356-run-edit-unhandled-rejection.done.md), [357](done/357-run-mkdir-silent-failure.done.md),
[358](done/358-extract-no-confirm-double-submit-guard.done.md), [359](done/359-rename-preview-case-sensitivity-mismatch.done.md),
[365](done/365-connections-dialog-no-busy-guard.done.md), [366](done/366-plugins-ui-raw-error-display.done.md),
[368](done/368-drive-select-redundant-refresh.done.md).

Reuse/efficiency cleanup:
[360](done/360-glob-to-regex-duplicated-converters.done.md), [361](done/361-crawler-bytes-recomputed-on-step.done.md),
[362](done/362-mtime-of-triplicated.done.md), [363](done/363-icon-mount-table-no-batch-cache.done.md),
[364](done/364-archive-is-link-mode-duplicated.done.md), [367](done/367-net-store-duplicated-authority-logic.done.md).

## Current implementation batch

Requested: 20 additional code tickets, excluding Windows/Linux-specific work.
Completed in this batch: **128, 137, 300, 107, 301, 303, 304, 125, 305, 126, 306, 134, 131 (13 of 20)**. New findings are recorded above.


## Review-fix batch (2026-09-25)

Completed: 307, 308, 309, 310, 311, 312, 313, 314, 315, 316.

## Rust advisory remediation (2026-09-25)

Nine remediation tickets cover all **18** crate/version/advisory findings
from ticket 110’s audit, without duplicate ownership. Related versions/advisories
are grouped where they share an upstream fix. P1/P2 are backlog priorities, not
CVSS ratings. Tickets 376–377 cover maintenance findings.

| Ticket | Remediation | Findings |
| --- | --- | --- |
| [369](done/369-h2-empty-data-frame-resource-exhaustion.done.md) | Upgrade h2 to bound empty HTTP/2 DATA frames | 1 |
| [370](done/370-rustls-handshake-encryption-level-boundaries.done.md) | Upgrade rustls to enforce TLS handshake encryption levels | 1 |
| [371](done/371-quick-xml-parser-resource-exhaustion.done.md) | Remove both quick-xml denial-of-service vulnerabilities from both dependency lines | 4 |
| [372](done/372-russh-and-cryptovec-allocation-bounds.done.md) | Fix SSH transport and CryptoVec allocation bounds | 2 |
| [373](done/373-rsa-marvin-timing-side-channel.done.md) | Remediate or prove mitigation for both RSA Marvin findings | 2 |
| [374](done/374-suppaftp-control-channel-command-injection.done.md) | Prevent FTP control-channel command injection | 1 |
| [375](done/375-time-parser-stack-exhaustion.done.md) | Remove the time parser stack-exhaustion finding | 1 |
| [376](376-replace-unmaintained-proc-macro-error.md) | Replace the unmaintained proc-macro-error dependency | 1 |
| [377](done/377-replace-unmaintained-unic-dependency-family.done.md) | Replace the five unmaintained rust-unic crates | 5 |

Use each ticket’s pinned audit evidence and verification criteria. Refresh the
audit before implementation; releases remain blocked until all findings have a
policy-valid disposition. No advisory exceptions were added by this backlog update.

## Code-fix batch (2026-09-25)

- [317](done/317-undo-eligibility-canonicalize-hard-error.done.md): completed and verified.

- [318](done/318-search-unreadable-files-not-counted.done.md): completed and verified.

- [320](done/320-ftp-webdav-missing-connection-timeout.done.md): completed and verified.

- [323](done/323-plugin-fs-preview-missing-capability-check.done.md): completed and verified.

- [324](done/324-checksums-missing-capability-check.done.md): completed and verified.

- [325](done/325-selection-save-load-missing-capability-check.done.md): completed and verified.

- [326](done/326-compare-dirs-aborts-on-single-entry-error.done.md): completed and verified.

- [327](done/327-filecompare-utf8-truncation-misclassification.done.md): completed and verified.

- [328](done/328-compare-dirs-stack-overflow-recursion.done.md): completed and verified.

- [329](done/329-search-index-stale-global-cache-epoch.done.md): completed and verified.

- [330](done/330-search-index-crawler-step-not-cancellable.done.md): completed and verified.

- [331](done/331-net-save-profile-monitor-stopped-before-validation.done.md): completed and verified.

- [332](done/332-probe-transport-missing-host-param.done.md): completed and verified.

- [333](done/333-monitor-unsupported-state-latched.done.md): completed and verified.

- [334](done/334-keepass-unlock-state-divergence.done.md): completed and verified.

- [335](done/335-cancel-token-map-orphan-leak.done.md): completed and verified.

- [336](done/336-fill-params-placeholder-resubstitution.done.md): completed and verified.

- [337](done/337-index-ipc-missing-hastauri-guard.done.md): completed and verified.

- [338](done/338-use-tauri-hastauri-not-reactive.done.md): completed and verified.

- [340](done/340-webdav-list-dot-dotdot-not-filtered.done.md): completed and verified.

- [342](done/342-mcp-mutex-lock-expect-poison-panic.done.md): completed and verified.

- [343](done/343-dir-size-blocking-call-not-spawn-blocking.done.md): completed and verified.

- [345](done/345-git-status-canonicalize-fallback-scope-widening.done.md): completed and verified.

- [346](done/346-dir-size-cancel-token-leak.done.md): completed and verified.

- [347](done/347-dir-size-generic-not-a-directory-error.done.md): completed and verified.

- [349](done/349-pack-sources-count-not-bounded-early.done.md): completed and verified.

- [350](done/350-archive-unknown-format-generic-error.done.md): completed and verified.

- [351](done/351-error-code-all-missing-tls-codes.done.md): completed and verified.

- [352](done/352-menu-language-apply-failure-silent.done.md): completed and verified.

- [353](done/353-probe-transport-unbounded-total-timeout.done.md): completed and verified.

- [355](done/355-normalize-associations-dead-code.done.md): completed and verified.

- [356](done/356-run-edit-unhandled-rejection.done.md): completed and verified.

- [357](done/357-run-mkdir-silent-failure.done.md): completed and verified.

- [358](done/358-extract-no-confirm-double-submit-guard.done.md): completed and verified.

- [360](done/360-glob-to-regex-duplicated-converters.done.md): completed and verified.

- [361](done/361-crawler-bytes-recomputed-on-step.done.md): completed and verified.

- [362](done/362-mtime-of-triplicated.done.md): completed and verified.

- [363](done/363-icon-mount-table-no-batch-cache.done.md): completed and verified.

- [364](done/364-archive-is-link-mode-duplicated.done.md): completed and verified.

- [365](done/365-connections-dialog-no-busy-guard.done.md): completed and verified.

- [366](done/366-plugins-ui-raw-error-display.done.md): completed and verified.

- [367](done/367-net-store-duplicated-authority-logic.done.md): completed and verified.

- [368](done/368-drive-select-redundant-refresh.done.md): completed and verified.

## Discovered while verifying the advisory batch (2026-09-27)

Running `npm run test:harness` for ticket 370 showed the protocol harness is
already red on `main` — 15 of 22 tests fail there — for two reasons that have
nothing to do with the advisory work:

- [378](done/378-webdav-collection-trailing-slash-redirect.done.md): WebDAV collection
  requests are not retried with a trailing slash, so every `mod_dav` server
  that answers `301` breaks listing, stat, mkdir, delete, staging and transfers.
  A product bug, not a harness bug.

- [379](done/379-harness-traversal-and-ca-test-defects.done.md): the traversal test
  unwraps a path the versioned locations now reject up front, and the
  certificate test lists an empty path because a `"/"` root trims to nothing —
  so neither test asserts what it claims.

With 378 and 379 fixed, 21 of 22 tests pass. The one that remains was found
while verifying 379 and is a third defect of the same family:

- [381](381-harness-local-paths-built-as-remote.md): the local↔remote transfer
  test wraps the local provider's own paths in `StreamPath::remote`, which
  `native()` refuses by design, so the local half of every such copy fails for
  all seven services.

## Advisory remediation outcome (2026-09-27)

Completed: **369, 370, 371, 372, 373, 374, 375, 377**. The live gate went from 18
findings to **2**. Two remain, each owned by an open ticket, and the release stays
blocked until they have a disposition:

- [376](376-replace-unmaintained-proc-macro-error.md): `proc-macro-error@1.0.4`,
  unmaintained, reached through the Linux GTK macro crates. Untouched — it needs a
  Linux build with real GTK/WebKit prerequisites.
- [380](done/380-rsa-marvin-successor-version.done.md): `rsa@0.10.0-rc.18`.
  Resolved 2026-09-27: SSH RSA signing was moved to AWS-LC, which leaves no
  reachable RSA private-key operation in the crate, and that control earns the
  gate's single bounded exception (expiring 2026-12-27). One finding still
  blocks release — 376's.

## Discovered in the 2026-09-28 bugfix review

Six findings from a full read of the transfer, archive, comparison, rename,
credential and WebDAV paths. None duplicates an earlier ticket; 382 and 384 are
the backend halves of frontend-only fixes (359, 326).

- [382](done/382-backend-rejects-case-only-renames.done.md) (P1, done): `rename_entry` and
  `rename_batch_on`'s pre-check refuse a case-only rename on macOS/Windows, so
  the rename ticket 359 made the preview allow still fails at execution.
- [383](383-webdav-cancelled-upload-still-puts.md) (P1): `WebdavWriter::drop`
  performs the `PUT`, so cancelling a WebDAV upload uploads the partial file
  and only then deletes it.
- [384](384-compare-aborts-on-unreadable-subdirectory.md) (P2): a
  `PermissionDenied` on any subdirectory aborts the whole recursive compare;
  the dialog then discards every batch it already received.
- [385](385-transfers-drop-timestamps-and-permissions.md) (P2): copies preserve
  no timestamp or mode — the `Provider` trait has no metadata operation — and
  `LocalProvider::limits` never answers `preserves_metadata`, so every plan
  carries a `metadataUnknown` advisory.
- [386](386-keepass-write-not-durable.md) (P2): the `.kdbx` write renames
  without syncing the parent directory, unlike every other publishing rename in
  the codebase.
- [387](387-extract-second-pass-name-handling.md) (P3): `extract_zip_one`
  slices `&name[base.len()..]` unchecked across a second archive open (panic),
  and 7z pass 1 normalizes separators while pass 2 does not.

Ticket 372 raised the declared MSRV from 1.85.1 to **1.93** deliberately: the
fixed releases for the quick-xml, suppaftp, time and urlpattern findings, and the
sspi release that lets a patched russh resolve at all, have no version below it.
