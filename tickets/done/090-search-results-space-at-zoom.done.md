# TICKET-090: Preserve readable search results at increased zoom

## Problem

Ticket 114's native macOS check at 200% page zoom, Large font and icon settings,
and a 1100×720 window found a search-results viewport only 39 native pixels high.
Each result occupies 80 native pixels at this zoom. The filename was partly
visible, but the parent path and selection cue were clipped. The form consumed
nearly all available height and could not scroll independently.

## Requirements

- Reserve room for complete result rows while allowing the query form to scroll.
- Keep title and result status visible and all form controls keyboard-reachable.
- Verify the real native search at 200% with Large font/icons in DE and EN,
  including reaching results, navigating to the last hit and revealing it.
- Run `npm run check`, document observed layout and locally commit this ticket.

## Scheduling

Urgent during ticket 114's required zoom and search workflow verification.

## Implementation and verification

The query form can shrink and scroll vertically. Title and result status retain
their height; the virtual result list reserves at least 80 CSS pixels, enough for
two of its 40-pixel rows. This changes only layout, preserving the existing search
and keyboard behavior.

`npm run check` passed on 2026-09-13 (387 Rust tests, 25 ignored; 488 frontend
tests; all formatting, lint, generated-binding, documentation, type, build and
bundle gates). The unsigned macOS test bundle was rebuilt with this change and
run with the isolated 043 configuration.

At actual 200% zoom, Large font/icons and a 1100×720 native window, the dialog
measured 1012×592. Its results area grew from 39 to 161 native pixels. A screenshot
confirmed two complete rows with filename, parent path and the selected-row
checkmark. The query form had its own scrollbar while title and hit count stayed
visible.

Both English and German `*.txt` searches used the local disposable file fixture.
Seven Tab presses from Name traversed Regex, Content/Inhalt, Regex,
Ignore/Ignorieren, Case/Groß-klein, Search/Suchen and the results. The focused
Search button scrolled into view at (1557,700), size 184×64 in English, and
(1550,700), size 191×64 in German. The results stayed 161 pixels high in both.
End and Enter revealed the last hit, `english-move.txt`; F2 exposed that same
filename and Escape cancelled without renaming. The English run searched the
fixture parent (three hits); the German run searched its left folder (two hits).

The setup also verified Large as selected for both size controls. In the 200%
settings dialog, DE/EN Tab traversal reached Export, Import and Reset, and Close
remained inside the dialog and returned focus to Settings. After testing, the
isolated app was set to German, Medium font/icons (selected values verified) and
100% zoom, with no dialog open. The broader 114 manual matrix remains open.
