# TICKET-354: Stop RowIcon/ColorRow subscribing reactively to the settings store

Discovered during a full-codebase review (2026-09-22).

## Problem

`RowIcon` (`src/components/ui/RowIcon.tsx`, lines 57-58), mounted once per
visible file row (`FileTable.tsx:537`, virtualized with `overscan: 12`) and
once per tree node (`DirTree.tsx:243`), subscribes reactively to the
*persisted* `useSettings` store (`useSettings(s => s.fileIcons)` plus
`useT()` → `useSettings(s => s.language)`) — exactly the pattern this
codebase already hit and fixed once for `dialogSizeStore` + `AppDialog` (see
the project's own memory of that incident).

Opening any folder mounts dozens of `RowIcon` instances simultaneously
(visible rows + overscan), each subscribing to the same persisted store; any
settings-store update or its persist rehydration firing while React is
mid-render of a sibling `RowIcon` reproduces "Cannot update a component while
rendering a different component."

The same anti-pattern exists at smaller scale in
`src/components/ui/settings/colors.tsx`: `ColorRow` (~15-20 instances) and
`CustomRuleRow` (one per user rule) each call `useT()` plus, in
`CustomRuleRow`, three more reactive `useSettings` action subscriptions, all
mounted simultaneously when the Colors settings page renders — lower
frequency/impact than `RowIcon` (page opened rarely, small item counts) but
worth fixing at the same time.

## Required change

- In `RowIcon`, read `fileIcons`/`language` via `useSettings.getState()`
  inside a lazy `useState` initializer or effect (or lift them once in
  `FileTable`/`DirTree` and pass as props) instead of subscribing reactively
  in the per-row component, following the same fix already applied to
  `dialogSizeStore`.
- In `colors.tsx`, hoist the shared `useT()`/action references once in
  `ColorsForm` and pass as props to `ColorRow`/`CustomRuleRow` (lower
  priority than `RowIcon`).

## Verification

- Manual/RTL test: open a large folder and trigger a settings-store update
  (e.g. language change) while rows are rendering; assert no "setState while
  rendering" warning.
- `npm run check`.
