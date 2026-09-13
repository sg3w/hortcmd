# Accessibility and localization

Ticket 043 is in progress. The rules below describe the implemented translation
check; the broader keyboard, screen-reader, zoom and live-region verification is
still being completed.

## Browser evidence (2026-09-13)

At a 640 × 360 CSS-pixel viewport, the settings dialog stayed inside the viewport
(x=32, y=18, width=576, height=324). With both app font and icon settings on Large,
German and English general settings remained accessible through the scrolling
form. In English, eleven Tab presses from the language control reached Export;
the form scrolled to 277 px and the focused button remained within the panel
(y=250–278), with a visible 2 px solid focus outline. Close remained in the fixed
footer. After dismissal, both file grids and all seven function-bar actions were
within the viewport; Quit wrapped to a second row ending at y=356. File-cell text
measured 14 px with the Large setting. Language, scales and viewport were restored
after inspection.

This is evidence for reduced viewport space and application scale. It does not
substitute for actual 200% browser zoom, operating-system display settings or
screen-reader speech tests, which remain pending below.

Contrast inspection found a default-theme failure tracked in ticket 084: selected
settings buttons render white 12 px text on `#3b82f6` in dark mode (3.6779:1).
Ticket 084 corrects this with separate foreground and surface tokens: white on
the new `#1d4ed8` fill measures 6.7016:1. Dark accent text uses `#bfdbfe` and light
accent text uses `#1d4ed8`; their default adjacent surfaces are regression-tested.
The browser confirmed the new fill in both themes. This verifies those token
pairs, not every custom file color or every other UI color.

## Native macOS evidence (2026-09-13)

The implementation at `bf34c60` was built as an unsigned debug app on macOS
26.6.2 (25G83), using the separate bundle identifier `com.hortcmd.a11ytest` and
`HORTCMD_CONFIG_DIR=/tmp/hortcmd-043-native/config`. The bundled app started and
exposed one window through System Events after accessibility access was granted.

In the German settings workflow, the test focused the native accessibility
element named Einstellungen and sent Enter. WebKit exposed the selected category
as an `AXRadioButton` titled Allgemein with value true. Arrow Down moved focus and
selection to Dateiansicht, also with value true. Escape closed the dialog and
restored focus to the Einstellungen button. This verifies native dialog key
handling and focus restoration; the initial focus was set through accessibility,
so this is not evidence for a complete Tab traversal of the application.

VoiceOver 10 was started through its first-run dialog. Reading its last phrase
through AppleScript did not yield usable output, so no screen-reader speech
result is claimed. VoiceOver was switched off again, its AppleScript-control
option was verified off (the original value), and the utility window was closed.
The manual matrix below remains pending; this native settings check does not
cover other workflows, English, actual zoom, or Windows/Linux screen readers.

## Translation ownership

German and English UI wording belongs in the typed resources under
`src/i18n/namespaces`. The existing dictionary tests enforce key parity, nonempty
values, identical placeholders and unique namespace ownership. Accessible names,
placeholders and titles follow the same rule as visible labels. File names,
addresses and file contents remain user data.

`npm run check:ui-text` parses production TSX with the TypeScript parser. It checks
literal JSX text, rendered literal/conditional/template expressions, and explicit
name/title/description/placeholder attributes. Translation calls and dynamic data
are not interpreted as literal UI wording. The checker distinguishes conditional
rendering guards from strings that are actually rendered.
It also checks named presentation properties in configuration objects, such as
option labels. Only exact keys found in the resource files are recognized as
translation references there; an arbitrary dotted string is not exempt. Direct
JSX rendering of a resource-key string is still reported. Language autonyms and
canonical Unix permission-bit names have explicit file/text exceptions.

This is a syntax-based regression gate, not a proof about arbitrary data flow.
Review must also inspect UI wording supplied through constants, configuration
objects, backend messages and custom component props. New component APIs must
keep presentation wording in typed translation resources.

`scripts/ui-text-exceptions.json` permits exact file/text pairs only, each with a
reason. The initial exceptions are the product name, technical size/rate units,
the Git tool name and an illustrative filesystem path. Unused exceptions fail the
check, so they cannot silently accumulate. Generic patterns and blanket file
exclusions are not permitted.

## Automated accessibility tests

`axe-core` is a development dependency and is not included in the production
bundle. The initial component checks cover the shared dialog shell and location
selector in both languages, failing on serious or critical WCAG violations.
The loaded application shell now has DE/EN axe checks as well. Its theme
integration test simulates system appearance events, verifies the rendered
`data-theme`, checks that an explicit selection overrides the system, and checks
listener cleanup on unmount. Only missing jsdom layout APIs are supplied; the
application, stores and theme effect run unchanged. This verifies the media-query
integration, not an actual operating-system settings change.
Tests also cover directory trees, tabs and all three virtualized file views in
both languages. Tree tests use the mocked native transport; file-view tests
render a scrolled window, verifying absolute row positions and total counts.
Virtualizer measurement is replaced only in jsdom, where layout is unavailable.
The additional screens and interaction coverage are described below.

File-operation keyboard tests exercise the real global handler, action dispatch,
domain clients and dialogs with the simulated Tauri transport in German and
English. F5/F6 send the marked file (distinct from the cursor) to the opposite
pane, F2 preserves the extension when replacing the preselected base name, and
F8/Escape/Enter cancel and confirm trash without dispatching permanent deletion.
An asynchronous rename failure exposes its localized description and restores
workspace focus when acknowledged. These tests verify command dispatch and UI
behavior; backend filesystem correctness remains covered by Rust tests.

The search workflow tests use Alt+F7, Tab, text entry, End and Enter with streamed
Tauri channel batches. They verify the selected result is revealed in its pane,
the dialog closes and workspace focus returns. A separate two-row virtual window
test navigates to result 1,000 and back while retaining focus and absolute result
positions. Search modes expose their pressed state; the result list uses listbox
and option semantics with a visible selection outline and check mark.
The search status region announces running and settled outcomes, including
cancellation and incomplete results. Streamed batches update the visible count
without mutating the live-region text. Tests observe that DOM boundary directly
and verify completion/cancellation announcements in both languages.

Settings categories form a vertical tab list with Arrow Up/Down and Home/End
activation and an associated tab panel. The category divider exposes its width
and accepts Left/Right, Home and End. Field groups use legends, segmented choices
expose pressed state, and switches separate their label from their explanatory
description. Tests traverse all eight pages with serious/critical axe checks in
German and English, change a theme by keyboard and check focus after dismissal.

Connection tests edit a saved profile, start a connection and navigate into its
returned address using keyboard input and simulated native commands. Error and
busy/cancel paths are covered in both languages. Each row is a named profile
group; disabled actions retain their action names while their tooltip explains
the busy state. Errors use an alert. These tests do not authenticate to a server;
native protocol and credential tests remain separate.

The remaining store-driven preview, properties and file-picker dialogs use the
shared workspace focus return. Six DE/EN regressions first reproduced body focus
after Escape and now verify focus on the original control. The text preview also
passes semantic axe checks and names its close action; its view choices expose
pressed state and its EXIF disclosure exposes expansion state.

The file picker uses a labelled listbox with a cursor that stays visible while
Arrow Up/Down and Home/End change rows. Enter opens folders or picks files;
selectable folders can be confirmed through the footer. The path input has a
localized name, and navigation errors use structured error text in an alert.
DE/EN tests navigate into a folder, pick a file or confirm a folder, inspect
selection semantics with axe and verify the callback path and focus return.

The jsdom tests disable color-contrast measurement because jsdom has no
painted layout. The isolated menubar and tab-bar fixtures additionally disable
the `region` rule because they omit the surrounding application landmarks; the
full-shell checks keep that rule enabled. Browser contrast, layout at 200% zoom, operating-system high
contrast, and manual screen-reader use require separate evidence; a green
semantic component test does not prove those properties.


## Implemented pane semantics

- Each pane has a localized tablist and a labelled tabpanel. Tab buttons use
  roving focus, arrow keys, Home/End and Delete; closing a tab restores focus to
  the new active tab. Close controls are outside the tablist's ARIA ownership.
- File views expose a multiselect grid. Details have column headers and cells;
  compact and thumbnail views expose their actual visual rows and columns.
  Virtual row indices and total counts include omitted rows. The active descendant
  represents the cursor; `aria-selected` represents independent file marks.
- Sort buttons accept native keyboard activation. Column separators expose their
  current width and support arrows in 10 px steps and Home/End for bounds.
- Directory trees expose level, sibling position/count, selection and expandable
  state. Up/Down, Left/Right, Home/End move the tree cursor; Enter opens a folder.
  Empty folders do not send Right to an unrelated sibling.
- Active-descendant references are emitted only while the target exists in the
  virtual window. Scrolling and screen-reader interaction still need browser QA.
- Controls receive a visible keyboard outline. Reduced-motion styling suppresses
  animation, and tab overflow scrolling respects the system motion preference.
  Forced-colors mode uses system colors, including with an explicit app theme.


## Operation and connection announcements

A persistent polite live region announces transfer state changes, including
background operations. Byte progress is sampled every ten seconds and announced
only if its rounded percentage changes. Paused, queued and completed operations
do not generate progress chatter; cancelling a queued operation is announced
even though it has no backend completion event. Transfer cards also show textual
completion/error states and expose named progress bars and pause state.

A separate polite live region announces connection lifecycle changes, preserving
localized failure messages. Identical backend status polls cause no live-region
DOM mutations. Initially disconnected saved profiles remain silent. A different
failure correlation ID is a new error even if the lifecycle phase stays failed.
Automated tests cover both languages, background progress throttling, immediate
state changes, queued cancellation and unchanged connection polling. Actual
assistive-technology speech remains part of the manual verification matrix.

## SSH terminal

The SSH terminal enables xterm's screen-reader mode and localizes its input name
and output-overflow message. Language changes do not reopen the shell. Escape
continues to reach the remote shell. Ctrl+Shift+F6 focuses the Close button, where
Enter closes the session; the shortcut is displayed in the terminal hint and
exposed through `aria-keyshortcuts`. Reduced motion disables cursor blinking.

Component tests use the native IPC stand-in and a minimal terminal adapter. They
verify that Escape reaches `ssh_send`, the exit chord does not, Close calls
`ssh_close`, and language changes preserve the same shell. They also check dialog
semantics with axe. These tests do not prove xterm's rendered accessibility buffer
or actual screen-reader speech; that requires the manual terminal workflow below.

## Manual verification matrix (not yet executed)

Record OS, application revision, screen-reader version, date, language, input
method and observed result for every run. Execute in both German and English:

| Platform | Native surface | Screen reader | Status |
| --- | --- | --- | --- |
| macOS | Tauri WebView and minimal native app menu | VoiceOver | Pending |
| Windows | Tauri WebView | NVDA | Pending |
| Linux | Tauri WebView | Orca | Pending |

1. Start with two disposable test folders and a few named files. Navigate both
   panes, mark multiple files, change sort and traverse tabs and the directory
   tree. Verify cursor and marks are distinct and virtualized positions are read.
2. Copy, move, rename and delete test files using only the keyboard. Cancel a
   prompt, resolve a name collision, then exercise a nested error prompt. Verify
   focus returns to the invoking control and Enter never opens an underlying file.
3. Search, open settings, switch language, increase font/icon scale, and close
   each dialog. Verify localized names, predictable Tab order and visible focus.
4. Navigate the menubar with arrows, Enter and Escape; inspect checked, disabled
   and overflow items. Verify menu typing does not start quick filtering. On macOS
   exercise native text editing and check that a shortcut runs exactly once.
5. Connect and disconnect a disposable remote profile, cause an authentication
   failure, and retry. Verify a single useful announcement per state change.
6. Start a transfer, minimize it, pause/resume, cancel and complete it. Listen for
   immediate state changes and progress no more frequently than every ten seconds.
7. Open SSH, read several output lines, type a harmless command, and send Escape.
   Use Ctrl+Shift+F6 and Enter to close. Verify output is navigable and focus returns.
8. Repeat representative dialogs and menus at actual 200% zoom and increased app
   scale. Check reduced motion, forced colors and system theme changes. Ensure
   actions remain reachable and status meaning does not depend on color alone.

Browser testing at 640 by 360 CSS pixels has verified function-bar wrapping,
horizontal access to detail columns and native Tab traversal. That is evidence
for a restricted viewport, not a substitute for the pending matrix or actual zoom.

## Theme and target requirements

Review built-in themes and custom file-color rules against each background they
can appear on: normal rows, alternate rows, cursor, marks, inactive pane, menus
and dialogs. Normal text must reach 4.5:1 contrast; large text may use 3:1. Evaluate
computed foreground/background colors, including opacity; do not round a failing
ratio up. Small filenames and status text use the normal-text threshold. Keep a
way to restore the default palette when users customize colors. See the
[W3C contrast guidance](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).

Pointer targets should occupy at least 24 by 24 CSS pixels. A smaller target needs
an explicit spacing or other applicable exception, recorded during review; the
icon's painted size is separate from its hit area. Do not enlarge invisible hit
areas so they overlap neighboring actions. Prefer larger targets when space
permits. Verify wrapped function bars, tab controls, tree expanders and resize
handles at increased scale. See the
[W3C target-size guidance](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html).

Marking now has a visible check as well as selection color and `aria-selected`.
Error notices use a warning triangle; ordinary results use an information icon.
Transfer completion, cancellation and errors have both icon and text. Color rules
remain optional emphasis; no action or file state may be discoverable through
color alone. A custom theme is not certified by the semantic axe tests: its
contrast and hit areas require rendered-browser measurements.

Pane result notices use a persistent polite live region, including repeated
identical results. Error notices remain until dismissed rather than expiring on
the information-notice timer. Menu tests exercise arrow navigation, Enter, typing
isolation, Escape focus restoration and axe checks in both languages.

## Main-window controls

The pane divider is a keyboard-focusable vertical separator. Left/Right change
the left pane by five percentage points, Home/End select the 15%/85% bounds.
Browser verification reached it through normal Tab order, changed 50% to 55%
(left pane measured 704 px at a 1280 px viewport), observed its 2 px focus outline,
and restored 50%. The command-line placeholder is named and disabled with an
unavailable explanation until ticket 085 implements command execution.
