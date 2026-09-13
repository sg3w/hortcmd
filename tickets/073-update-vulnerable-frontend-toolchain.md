# TICKET-073: Update dependencies reported by the npm security audit

## Finding
Installing the axe development dependency during 043 exposed ten audit findings
in the existing dependency tree (2026-09-13). The critical report concerns Vitest
UI server file access/execution (GHSA-5xrq-8626-4rwp); the ordinary `vitest run`
command does not start that UI server. Vite has development-server path handling
findings, and PostCSS/Browserslist have build-time input handling findings.
DOMPurify reports custom-element and in-place hook issues; the preview currently
uses an explicit allowlist and afterSanitizeAttributes rather than either reported
configuration. Verify each advisory against the actual configuration during fixes.

## Work
- Upgrade to fixed compatible versions, with deliberate Vite/Vitest migration
  rather than an unchecked major `npm audit fix --force`.
- Preserve the Node/MSRV and plugin constraints recorded in the project.
- Re-run the security audit, preview sanitization tests and `npm run check`.
- Document any residual advisory with actual reachability and mitigation.

## Scheduling
Before starting interactive development-server/UI-server verification for 043.
Headless component tests and source work can proceed without those servers.
This is a concrete dependency finding; 050 still owns ongoing governance.
