# Ticket organization

All tickets below 100 and tickets 100–103 have been completed.
The remaining **34 open tickets run from 104 to 137**. Their relative order
and scope are unchanged. Dependencies and references use the current paths.

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
| 205 | [104](104-plugin-context-handoff.md) | Context handoff to plugins |
| 206 | [105](105-plugin-actions-events.md) | Mediated actions & events |
| 207 | [106](106-plugin-management.md) | Plugin management (settings) |
| 208 | [107](107-reference-demo-plugin.md) | Reference/demo plugin(s) |
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
| 261 | [125](125-index-search-session-restore.md) | Indexsuche nach Dialog-Unmount konsistent wiederherstellen |
| 262 | [126](126-archive-navigation-history.md) | Archiv-Orte im Navigationsverlauf erhalten |
| 263 | [127](127-native-paired-comparison-verification.md) | Native Prüfung der doppelten Vergleichsliste |
| 264 | [128](128-rename-dialog-structured-errors.md) | Strukturierte Batch-Fehler im Umbenennen-Dialog anzeigen |
| 265 | [129](129-windows-linux-native-file-icons.md) | Native Dateisymbole unter Windows und Linux |
| 266 | [130](130-native-icon-gui-verification.md) | Native GUI-Prüfung der Dateisymbole ergänzen |
| 267 | [131](131-zip-lzma-framing-conformance.md) | ZIP-LZMA-Framing mit externen Archiven prüfen |
| 268 | [132](132-archive-budget-upstream-migration.md) | Lokale Archiv-Budgetpatches upstream ersetzen |
| 270 | [133](133-atomic-provider-staging-ownership.md) | Reserve provider staging entries atomically and verify ownership |
| 273 | [134](134-jsdom-canvas-test-diagnostic.md) | Fehlende Canvas-Unterstützung im Frontend-Testaufbau klären |
| 274 | [135](135-command-line-native-verification.md) | Native Windows/Linux command-line verification |
| 275 | [136](136-native-trash-error-verification.md) | Verify native trash errors and refused Finder automation |
| 276 | [137](137-lazy-feature-translations.md) | Load feature translations with their lazy UI |

## Earlier moves

Older history may describe the prior scheduling: 056 became 209 (now 108),
045 became 220 (now 109), 050 became 221 (now 110), 051 became 222 (now 111),
052 became 223 (now 112), 046 became 225 (now 113), and 043 became 249 (now 114).
074 became [224](done/224-vitest-module-mock-hoisting.done.md), which is already
completed and retains that number. The table above preserves the identity of
historical references while active links use the current filenames.
