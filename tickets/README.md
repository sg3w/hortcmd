# Ticket organization

All tickets below 100 and tickets 100–107 have been completed.
Tickets 125–126, 128, 131, 134, 137 and the discovered issues 300–301 and 303–306 are also complete. There are
**24 open tickets**: 108–124, 127, 129–130, 132–133 and 135–136. Dependencies use the current paths.

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

## Current implementation batch

Requested: 20 additional code tickets, excluding Windows/Linux-specific work.
Completed in this batch: **128, 137, 300, 107, 301, 303, 304, 125, 305, 126, 306, 134, 131 (13 of 20)**. New findings are recorded above.

