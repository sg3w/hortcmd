# TICKET-069: Isolate MCP pane resource tests

## Finding
The full check reproduced a race between `a_pane_nobody_has_published_reads_as_unavailable`
and `a_pane_outside_the_roots_is_named_without_its_contents`: both mutate the global
pane snapshot, so the former can see the latter's published pane. The individual
MCP test run passed; the full parallel Rust suite exposed the failure.

## Acceptance
- Serialize the resource tests that mutate this shared snapshot without disabling
  parallel execution of unrelated tests or changing production behavior.
- The full `npm run check` passes.

Completed: the pane-mutating tests share a dedicated PANE_TEST mutex; unrelated
tests remain parallel. Implementation is committed in 40f8f9f together with the
new 041 module, avoiding a deliberately racy intermediate revision. Repeated full
checks passed; no production behavior was changed for test synchronization.
