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

## Additional compatibility finding
The pre-existing jsdom 30.0.1 lock entry required Node 22.22.2/24.15/26 despite
README advertising Node 20; Jest-DOM 7 also required Node 22. The update uses
jsdom 29.1.1 and the Node-20-compatible Jest-DOM 6.9.1 line, declares supported
Node ranges explicitly, and adds a Node 20 frontend CI job. Rust dependencies and
the Rust MSRV are unchanged.

## Implemented versions and verification
- Vite 6.4.3, Vitest 4.1.11, PostCSS 8.5.28 and DOMPurify 3.4.15.
- Browserslist 4.28.9 and baseline-browser-mapping 2.11.23 resolve the remaining
  audit findings. Final npm audit reports zero vulnerabilities.
- jsdom 29.1.1 supports the declared Node 20.19 / 22.13 / 24+ ranges; the existing
  React Vite plugin remains compatible. A Node 20 frontend CI job is added.
- The complete `npm run check` passed on the official Node 20.19.0 runtime,
  including preview sanitization tests, production build and bundle limits.
  Its archive checksum was verified against the official nodejs.org manifest.
- The exact staged 073 tree was exported without any unfinished 043 changes.
  Its fresh `npm ci --engine-strict --ignore-scripts` on Node 20.19 passed with
  zero audit findings; all 378 baseline frontend tests, TypeScript, build and
  bundle checks passed independently.
- The working-tree full check also passed with the additional 043 tests present:
  387 Rust tests passed, 25 ignored, 382 frontend tests passed. No source or test
  changes from 043 are included in this dependency commit.
