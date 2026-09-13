# Ticket organization

## Current scheduling

The current batch covers ten bug-fix or feature tickets. Windows/Linux-specific
work and supporting tasks remain deferred. Renumbering does not complete tickets.

- Direct development retains its existing numbers: user-facing file-manager
  features, backend functionality, data safety and concrete application defects.
  This includes transfer recovery, archive/transport safety, undo, preflight,
  caching, indexed search, the VFS model and the outstanding interaction fixes.
- 201–209 cover the plugin interface, hosts, integration, technical examples and
  the first functional plugin. The storage analyzer belongs to 209.
- 220 and above contain supporting work: support diagnostics, governance,
  releases, benchmark infrastructure, test maintenance and cross-cutting quality
  verification. Previously assigned 249 and 250 retain their numbers.
- Completed tickets keep their numbers. Dependencies remain valid after moves;
  a higher number does not waive required safety checks or feature-specific tests.

## Renumbered open tickets

| Previous | Current | Scope |
| --- | --- | --- |
| 056 | [209](209-folder-storage-usage-visualizer.md) | First functional plugin: storage-usage analyzer |
| 045 | [220](220-redacted-diagnostics-and-support-bundle.md) | Diagnostics and support bundles |
| 050 | [221](221-dependency-and-license-governance.md) | Dependency and license governance |
| 051 | [222](222-signed-reproducible-releases-and-updates.md) | Signed releases and safe updates |
| 052 | [223](223-performance-benchmarks-and-budgets.md) | Performance benchmarks and budgets |
| 074 | [224](224-vitest-module-mock-hoisting.md) | Test mock maintenance |
| 046 | [225](225-safe-archive-extraction.md) | Remaining archive platform/security audit; implementation already present |
| 043 | [249](249-accessibility-and-localization-quality.md) | Accessibility and localization quality; previously moved |

[250](250-windows-linux-screen-reader-verification.md) remains the Windows/NVDA
and Linux/Orca follow-up. The outstanding macOS evidence remains in 249.

[251](251-windows-linux-transfer-recovery.md) tracks native Windows/Linux transfer
recovery enablement and verification deferred from 044.

[252](252-native-transport-security-verification.md) tracks the native transport
security and live SMB validation matrix deferred from 047.

[253](253-negotiated-tls-diagnostics.md) tracks additional negotiated TLS telemetry
that the current client APIs do not expose.
