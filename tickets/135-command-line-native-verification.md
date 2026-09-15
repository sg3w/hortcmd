# TICKET-135: Native Windows/Linux command-line verification

## Problem

Ticket 085 adds non-interactive local command execution. macOS execution and
frontend keyboard tests run locally; native Windows and Linux environments were
not available. Cross-platform verification is deferred per project priorities.

## Requirements

- On Windows verify cmd.exe /D /S /C quoting, Unicode/legacy output encodings,
  paths with spaces, nonzero exit, closed stdin and taskkill /T /F cancellation.
- On Linux verify /bin/sh behavior, dedicated process groups, descendants,
  cancellation and application shutdown.
- On both verify large stdout/stderr, pane changes while running, background
  descendants, inaccessible folders and output keyboard accessibility.
- Record platform/version, commands, expected and observed results; fix failures
  on the ticket branch and run npm run check.
