# TICKET-137: Load feature translations with their lazy UI

## Problem

Ticket 086's two new bilingual error messages grow the eager entry from 319.8 to
320.5 KiB. Its documented budget rises to 321 KiB to keep immediate operation-error
messages available. The shared dictionary also contains feature-only translations
whose dialogs already load lazily; future feature growth will consume this small
remaining margin again.

## Requirements

- Identify translations used exclusively by lazy features and load them alongside
  those features. Keep core operation/error messages immediately available.
- Preserve compile-time key checking, German/English completeness and runtime
  language switching; avoid duplicate dictionaries or a flash of untranslated keys.
- Compare entry size and total eager bytes before/after. Do not merely move the
  same eagerly loaded bytes into a different named chunk to evade the entry limit.
- Add focused localization/loading regressions and run npm run check.


## Implementation evidence

Rename, file-list export/selection, and preview namespaces now register both
languages with their lazy feature modules before rendering. The core imports
only their types, retaining the complete checked TransKey union. Mixed-use
namespaces and operation/error text remain eager. There is one runtime registry,
not a second copy of feature dictionaries.

Before: entry 323.7 KiB, complete eager graph 799.2 KiB.
After: entry 316.8 KiB, complete eager graph 792.2 KiB.
The 325 KiB entry budget is unchanged. The bundle gate now also checks that
representative feature-only keys are absent from the entire eager import graph.

`npm run check` passed, including 702 frontend tests. Focused regressions verify
that text is absent before feature loading, present before the loader resolves,
and switches language without fetching again or displaying untranslated keys.
