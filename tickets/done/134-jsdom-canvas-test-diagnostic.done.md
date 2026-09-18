# TICKET-134: Isolate the missing canvas diagnostic in frontend tests

## Finding

The lazy-dialog loader test imports real xterm. Its module-level color helper
probes a 1px canvas with `getContext("2d", { willReadFrequently: true })`.
jsdom returns no context and emits its missing-canvas diagnostic. The source was
confirmed using the original jsdom error stack and reproduced with only the
lazy-dialog test. It was not a production error or a failing accessibility check.

## Resolution

The loader test installs a local `getContext` adapter returning `null` only while
importing the terminal module. It asserts that the real color-helper probe ran,
checks the real component export and restores the method in `finally`.
No console suppression, global canvas substitute or accessibility rule change
is introduced. Existing terminal keyboard and screen-reader tests remain active.
The shared setup contains no diagnostic instrumentation.

## Verification

The focused loader and terminal tests pass without the canvas warning.
Validation passed: `npm run check`, including all 746 frontend tests with no
missing-canvas diagnostic and the production build within budget.
