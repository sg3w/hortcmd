# Ticket organization

All tickets below 100 and tickets 100–107 have been completed.
Tickets 125–126, 128, 131, 134, 137 and the discovered issues 300–301 and 303–306 are also complete. There are
**79 open tickets**: 108–124, 127, 129–130, 132–133, 135–136 and the discovered
issues 314–368. Dependencies use the current paths.

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
| 209 | [108](108-folder-storage-usage-visualizer.md) | First functional plugin — folder storage-usage visualizer |
| 220 | [109](109-redacted-diagnostics-and-support-bundle.md) | Add redacted diagnostics and an exportable support bundle |
| 221 | [110](110-dependency-and-license-governance.md) | Establish dependency, vulnerability, and license governance |
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
[313](done/313-discard-staging-remote-destination-noop.done.md), [314](314-transfer-commit-rollback-failure-swallowed.md),
[315](315-parallel-copy-orphaned-directories.md), [316](316-zip-directory-entries-skip-manifest-verify.md),
[317](317-undo-eligibility-canonicalize-hard-error.md), [318](318-search-unreadable-files-not-counted.md).

Correctness (backend):
[319](319-smb-remove-dir-all-reparse-point-recursion.md), [320](320-ftp-webdav-missing-connection-timeout.md),
[321](321-vfs-listing-unbounded-memory-allocation.md), [322](322-native-plugin-windows-orphan-process.md),
[323](323-plugin-fs-preview-missing-capability-check.md), [324](324-checksums-missing-capability-check.md),
[325](325-selection-save-load-missing-capability-check.md), [326](326-compare-dirs-aborts-on-single-entry-error.md),
[327](327-filecompare-utf8-truncation-misclassification.md), [328](328-compare-dirs-stack-overflow-recursion.md),
[329](329-search-index-stale-global-cache-epoch.md), [330](330-search-index-crawler-step-not-cancellable.md),
[331](331-net-save-profile-monitor-stopped-before-validation.md), [332](332-probe-transport-missing-host-param.md),
[333](333-monitor-unsupported-state-latched.md), [334](334-keepass-unlock-state-divergence.md),
[335](335-cancel-token-map-orphan-leak.md), [339](339-smb-ensure-share-race-condition.md),
[340](340-webdav-list-dot-dotdot-not-filtered.md), [341](341-smb-shares-error-generic-code.md),
[342](342-mcp-mutex-lock-expect-poison-panic.md), [343](343-dir-size-blocking-call-not-spawn-blocking.md),
[344](344-finder-tags-dead-fallback-code.md), [345](345-git-status-canonicalize-fallback-scope-widening.md),
[346](346-dir-size-cancel-token-leak.md), [347](347-dir-size-generic-not-a-directory-error.md),
[349](349-pack-sources-count-not-bounded-early.md), [350](350-archive-unknown-format-generic-error.md),
[351](351-error-code-all-missing-tls-codes.md), [352](352-menu-language-apply-failure-silent.md),
[353](353-probe-transport-unbounded-total-timeout.md).

Correctness (frontend):
[336](336-fill-params-placeholder-resubstitution.md), [337](337-index-ipc-missing-hastauri-guard.md),
[338](338-use-tauri-hastauri-not-reactive.md), [348](348-progress-overlay-no-focus-trap.md),
[354](354-reactive-settings-subscription-bulk-mount.md), [355](355-normalize-associations-dead-code.md),
[356](356-run-edit-unhandled-rejection.md), [357](357-run-mkdir-silent-failure.md),
[358](358-extract-no-confirm-double-submit-guard.md), [359](359-rename-preview-case-sensitivity-mismatch.md),
[365](365-connections-dialog-no-busy-guard.md), [366](366-plugins-ui-raw-error-display.md),
[368](368-drive-select-redundant-refresh.md).

Reuse/efficiency cleanup:
[360](360-glob-to-regex-duplicated-converters.md), [361](361-crawler-bytes-recomputed-on-step.md),
[362](362-mtime-of-triplicated.md), [363](363-icon-mount-table-no-batch-cache.md),
[364](364-archive-is-link-mode-duplicated.md), [367](367-net-store-duplicated-authority-logic.md).

## Current implementation batch

Requested: 20 additional code tickets, excluding Windows/Linux-specific work.
Completed in this batch: **128, 137, 300, 107, 301, 303, 304, 125, 305, 126, 306, 134, 131 (13 of 20)**. New findings are recorded above.


## Review-fix batch (2026-09-25)

Completed: 307, 308, 309, 310, 311, 312, 313.
