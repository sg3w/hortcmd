# TICKET-133: Reserve provider staging entries atomically and verify ownership

## Finding
During TICKET-072, inspection found that the shared transfer planner checks a
working name for absence before the provider creates it. Provider `create_write`
and `create_dir_all` do not expose a uniform exclusive reservation operation.
Another process can race creation or replace the entry before cleanup. This is a
pre-existing limitation of shared staging, including ordinary non-MCP transfers.
MCP now restricts cancellation cleanup to its registered, successfully created
staging root, but this logical ownership is not an external replacement guard.

## Scope
- Introduce provider capabilities for exclusive staging allocation and stable
  ownership validation; use private staging containers where appropriate.
- Never truncate or remove an entry inserted/replaced by another actor.
- Refuse unsafe cleanup and report the leftover when identity cannot be verified.
- Exercise concurrent creation/replacement races without relaxing MCP live scope
  checks, cancellation checks on publication, or source deletion ordering.

## Priority
Data-safety hardening of shared transfer staging; separate from cancellation
cleanup in TICKET-072.
