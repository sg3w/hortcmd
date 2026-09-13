# TICKET-066: Harden MCP scope and session approvals

## Priority
Urgent; discovered while completing 041. Must be resolved before enabling MCP.

## Findings
- Local scope case folding assumes macOS volumes are case insensitive, which can admit a sibling on a case-sensitive volume.
- A dangling symlink is treated as a missing destination; creating its target later can escape the root.
- Remote scope uses lexical paths without proving server-side symlink containment.
- Remembered confirmations use a self-reported client name and accept unknown question IDs.
- Scope must be checked again after a confirmation wait; rename must verify both providers identify the same store.

## Acceptance
- [x] Canonical local paths use filesystem-resolved spelling and reject dangling symlinks.
- [x] Remote scope verifies canonical containment or fails closed when unavailable.
- [x] Approvals are keyed by a server-issued session identity; unknown answers cannot grant permissions.
- [x] Scope is revalidated after confirmation and cross-store rename is refused.
- [x] Regression tests and npm run check pass.

Production fixes are integrated in 041 commit 40f8f9f so that no insecure
intermediate server is committed. This ticket adds the dedicated canonical-alias
regression and records closure. Real remote validation passed all seven protocols
in harness 599e21; token rotation is covered by the HTTP regression in 041.

Additional integration findings: local scope must compare canonical namespaces
without a lexical prefilter (`/var` and `/private/var` are legitimate aliases on
macOS). Token rotation must reject old authenticated requests before acquiring or
creating sessions; a generation check closes the race with session revocation.
