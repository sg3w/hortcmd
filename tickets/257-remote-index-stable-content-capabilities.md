# TICKET-257: Prove remote no-follow and stable-content reads for optional indexing

Discovered while implementing TICKET-054. Current remote VFS APIs expose directory
metadata but cannot uniformly prove a no-follow, stable content read or physical
mount identity. TICKET-054 therefore accepts explicit remote metadata scopes only
and rejects remote content opt-in before creating a scope.

## Goal

Add provider capabilities and implementations that safely enable optional remote
content indexing without widening approved paths or storing data from raced links.

## Acceptance Criteria

- [ ] Capabilities distinguish stable handles, link policy and mount/namespace boundaries.
- [ ] Content reads verify object/version before and after, with cancellation and size limits.
- [ ] Disconnect, reconnect, trust change and raced-link tests prove isolation.
- [ ] Unsupported providers retain the explicit metadata-only policy.
- [ ] Privacy text and index contract tests cover every newly enabled provider.

This can follow 054: the existing rejection prevents the unsafe operation now.
