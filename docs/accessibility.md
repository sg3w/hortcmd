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

VoiceOver 10 was started through its first-run dialog. An initial attempt to read
its last phrase failed. A subsequent run explicitly enabled its supported
AppleScript-control option and verified that option through a fresh utility
window reference before starting the screen reader. VoiceOver then returned
these actual output phrases for the native German settings workflow:

- Enter: `Einstellungen web dialog mit 5 Objekten Dateiansicht ausgewählt tab, 2 von 8 Einstellungen tab group`
- Arrow Up: `Allgemein ausgewählt tab, 1 von 8`
- Escape: `Einstellungen button`

The category persisted from the earlier test, so this run started on Dateiansicht.
The phrases establish VoiceOver's dialog, selection/position and returned-focus
announcements for this workflow. They are screen-reader output text, not a
recording or an assessment of audio intelligibility. VoiceOver was switched off
again, its AppleScript-control option was verified off (the original value), and
the utility window was closed. The complete manual matrix below remains pending;
this check does not cover other workflows, actual zoom, or Windows/Linux
screen readers.

A subsequent English settings run captured the following VoiceOver output after
its startup announcement had finished. The screen reader's system language
remained German, while the application's labels were English:

- Enter: `Settings web dialog mit 5 Objekten File view ausgewählt tab, 2 von 8 Settings tab group`
- Arrow Up: `General ausgewählt tab, 1 von 8`
- Escape: `Settings button`

The application language was then restored to German, verified by the returned
focus name Einstellungen. VoiceOver was quit after the test. Disabling its
temporary AppleScript-control option presented a macOS password confirmation.
A subsequent read confirmed that VoiceOver was not running and the option was
off (value 0); the utility window was then closed.

Additional German native keyboard checks used System Events against the bundled
app, with VoiceOver off:

- From the in-window Hilfe menu trigger, Arrow Right wrapped to Dateien; Arrow
  Down opened that menu on Öffnen. Escape returned focus to Dateien. No menu
  command was executed.
- Tab moved from that menu trigger to the enabled Übergeordneter Ordner toolbar
  button. F7 opened the new-folder prompt and focused its text field.
- After typing `hortcmd-a11y-cancel-only`, Cmd+A exposed that complete string as
  the native `AXSelectedText`. Typing `hortcmd-a11y-replacement` replaced the
  entire value. Escape cancelled the prompt and returned focus to Übergeordneter
  Ordner. No folder creation was submitted.

These checks cover native menu navigation, one toolbar Tab transition, macOS text
selection/replacement, prompt cancellation and focus restoration. They do not
establish completion of other filesystem-operation or screen-reader workflows.

Native German file-operation checks used a fresh directory under `/private/tmp`,
with separate `left` and `right` folders and one `original.txt` fixture. Both panes
were navigated through their native breadcrumb controls and keyboard filtering;
their full displayed paths were checked before operations. Long synthetic text
bursts were not reliable in the large temporary directory, so the verified
navigation sent individual characters and checked the resulting filter/path.

- F2 exposed `original.txt` with only `original` selected. Replacing that selection
  produced `renamed.txt`; Enter renamed the actual file without changing its bytes.
- F5 copied `renamed.txt` into the opposite test pane. Both files existed and had
  the same SHA-256 digest.
- The source was renamed to `moved.txt` through F2, then moved with F6. The source
  directory was empty afterward; both destination files retained the fixture's
  SHA-256 digest `1fd20dfd9c7ac5b8f330f552ba5d4be2a7db59d88f9eaff8f78b88d1dc52d004`.
- F8 opened the trash confirmation. Escape returned native focus to a file row,
  and both destination files remained present. A subsequent Enter confirmation
  for `moved.txt` reached a macOS Finder-automation permission prompt and later
  ended in a localized Lese-/Schreibfehler dialog. Both destination files remained
  intact. Enter acknowledged that error and restored native focus to a file row.
  The underlying cause is not established; ticket 086 tracks the blanket trash
  error mapping. Successful trash disposal has not yet been verified.

These are native keyboard and filesystem observations with VoiceOver off. They
do not substitute for the remaining screen-reader, English-operation or zoom
checks. The permission prompt names ChatGPT, the host of the launched test process;
this does not establish the permission presentation of a separately installed app.

The native German search test started from the same right-hand fixture directory
with Alt+F7. Tab traversed the four search modes, Ordner and Name; entering
`*.txt` in Name and pressing Enter ran the actual Rust search. The root field
retained the fixture directory, and the accessible result list contained exactly
`moved.txt` and `renamed.txt`, each with its full parent path and 45-byte size.
From Name, seven Tab presses traversed Regex, Inhalt, Regex, Ignorieren,
Groß/klein and Suchen before reaching the first result. End moved the native
focused result to `renamed.txt`; Enter closed the search and returned focus to
a file row. Opening F2 then exposed `renamed.txt`, confirming the revealed cursor
target; that verification prompt was cancelled without editing the file. This
proves the native search/keyboard/reveal path in German with VoiceOver off.

English native checks repeated the file and search flows in those same disposable
folders. The Rename dialog selected only `moved` in `moved.txt`; replacing it with
`english` retained the extension. F5 copied `english.txt` from right to left with
identical hashes. Renaming the source to `english-move.txt` and using F6 removed
that source while preserving its bytes at the left-hand destination. The remaining
right-hand `renamed.txt` stayed present after F8/Escape; the dialog read Move to
trash and its message explicitly distinguished permanent deletion with Shift.

Alt+F7 opened Search on the left-hand fixture folder. Entering `*.txt` and pressing
Enter produced an accessible hits list containing `english.txt` and
`english-move.txt`, each 45 bytes with the correct parent path. From Name, Tab
traversed Regex, Content, Regex, Ignore, Case and Search to the first result. End
selected `english-move.txt`; Enter returned focus to a file row, and an immediately
cancelled F2 prompt verified that exact revealed filename. The app was restored
to German afterward, confirmed by the Einstellungen focus name. These tests ran
with VoiceOver off and did not retry the previously unsuccessful trash operation.

Native connection and terminal checks used the repository's disposable protocol
harness (run `c6442c`), with all seven containers healthy and published only on
loopback. A new SFTP profile was filled through named native controls and Tab
navigation, saved with the generated test credentials, and connected to the
local SSH server. The unknown-host prompt's address and SHA-256 fingerprint were
matched against the harness's generated public host key before using Vertrauen
und verbinden. The left pane then displayed the actual SFTP `/data` directory.

In German, arrows and Enter in Befehle opened Terminal im aktuellen Ordner öffnen;
native focus became SSH-Terminaleingabe. Sending an echo command produced both
the echoed command and a separate exact `HORTCMD_NATIVE_TERMINAL_OK` output line
in xterm's native accessibility list. Escape retained terminal input focus;
Ctrl+Shift+F6 focused Schließen, and Enter closed the terminal and restored focus
to Befehle. Enter on Trennen changed the profile action back to Verbinden.

The English run reconnected with Connect and loaded the same SFTP path. Commands
and Open terminal in current folder opened SSH terminal input; a separate exact
`HORTCMD_NATIVE_EN_OK` server-output line was verified. Escape retained input,
Ctrl+Shift+F6 reached Close, and Enter returned focus to Commands. Disconnect
restored the Connect action. These checks used real SSH traffic and the rendered
xterm accessibility buffer with VoiceOver off; they do not prove spoken terminal
output or the Windows/Linux matrix.

Cleanup removed the test host key through Drop host key and deleted the test
profile through the UI. The isolated profile list was empty, its managed host-key
file was empty, and an exact service/account keychain lookup returned item-not-found
(44), without reading a password. The private input script, harness containers and
run directory were removed. The app returned to German and the previous local
fixture directory. No test credentials are included in these test notes.

## Native page zoom (ticket 088)

The native View menu provides Zoom in, Zoom out and Actual size (100%). Use Cmd
on macOS or Ctrl on Windows/Linux with main-keyboard Plus/Equals, Minus or Zero.
Zoom changes in 25-percentage-point steps between 50% and 300%; Zero restores
100%. It works inside dialogs and terminals without sending the zoom key to a
remote shell. Numeric-keypad selection shortcuts retain their existing meaning.
Browser builds use their browser's own zoom controls. Font/icon preferences are
independent of page zoom; page zoom starts at 100% on launch.

The 2026-09-13 macOS test bundle with ticket 088 changes demonstrated actual
native enlargement at a fixed 1100×720 window size: the Settings toolbar button
grew from 25×25 to 50×50 at 200%. The View-menu command and physical Cmd-Plus on
the German keyboard both changed zoom. Settings opened at 200%, and Cmd-Minus
and Cmd-0 worked within it while General kept focus. Escape returned to Settings
at 25×25 after reset. The test left the app at 100% with the dialog closed.
This proves native page zoom and these interactions; the complete layout,
screen-reader and Windows/Linux matrix below remains pending.

Ticket 089's native 200% toolbar check found and fixed document-wide horizontal
scrolling when focus reached the offscreen Settings button. The toolbar now has
its own horizontal scroll area. Twelve Tab presses from Up reached Settings,
fully visible at 50×50; Shift-Tab returned to Up. The Files menu kept exactly the
same position (746,368) throughout, whereas the unfixed version had moved it
outside the window to x=692. The toolbar remains a single row, and the screenshot
confirmed its independent scrollbar and unchanged pane split. Zoom was reset to
100% afterwards. This verifies this toolbar overflow path, not the entire matrix.

Ticket 090's native search check combined 200% zoom with Large font and icon
settings in the same 1100×720 window. Before the fix, the hit viewport was only
39 native pixels high and clipped the first result's parent path. The query form
now scrolls independently, reserving two complete result rows. The corrected
viewport measured 161 pixels in DE and EN; a screenshot verified both rows,
paths and the non-color selection checkmark. Tab reached all query controls and
the results, including the Search/Suchen button after it scrolled into view.
End and Enter revealed `english-move.txt` in both languages; an F2 inspection
confirmed its name and was cancelled with Escape without renaming.

The same setup checked the settings dialog at 200% (990×620, within the window).
With both size controls explicitly verified Large, Tab reached Export, Import
and Reset in DE and EN; the Close button remained visible and returned focus to
Settings. Afterwards the isolated app was set to German, Medium font/icons and
100% zoom, verified through native control values. Other workflows and the full
screen-reader matrix remain pending.

Ticket 091 checked native overflow menus at 200% in a narrower 700×720 window.
The DE Commands submenu initially extended outside the window (x=396 against a
window left edge of x=730). Adaptive overlapping now places it entirely inside,
at (840,433), size 564×542; a screenshot verified readable labels. Further
navigation through Special search and back with Left/Escape preserved focus.
The EN View → Layout submenu also remained inside the viewport, including while
the window widened to 760 pixels, and returned focus through the same keys.
At 1100×720 and 100%, adjacent submenu columns still worked. The test restored
German, 100% and the original window size without invoking a menu command.

At revision `0faa78e`, native DE/EN rename and trash-confirmation dialogs were
checked at 200% in the 1100×720 window. F2 opened `english-move.txt` with only
`english-move` selected, preserving the extension. The name input measured
772×60 native pixels; both action buttons were inside the window. Tab reached
Cancel/Abbrechen, and Enter cancelled. F8 then opened the trash confirmation;
Shift-Tab reached Cancel/Abbrechen and Enter cancelled that as well. In German,
these actions were invoked from the file grid and returned to an `AXRow`; in
English they were invoked while the Settings toolbar button held focus and
returned to Settings. The English trash-dialog screenshot confirmed the complete
message, permanent-delete hint, action and Cancel button within the window.

All three fixture filenames and SHA-256 content hashes matched before and after
the checks. The app was returned to German and 100% with no dialog open. This
establishes these zoomed dialog and cancellation paths; it does not establish a
successful native trash operation or additional screen-reader speech coverage.

## Native system appearance check (2026-09-13)

With the native app at revision `0faa78e`, the initial macOS appearance was Dark
and the app explicitly used Dark. The isolated app was changed to System, then
macOS was switched to Light and back to Dark without restarting hortcmd. Native
window screenshots showed the file panes, menus, toolbar, status and function
bar adopting the light palette and then returning to the dark palette. The
System control remained selected throughout the OS changes.

Both original values were restored and checked: macOS Dark was true and the
app's Dark control was selected. The app remained in German at 100% zoom with no
dialog open. This verifies live native system-theme changes; reduced-motion,
high-contrast and additional screen-reader checks are separate requirements.

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

## Manual verification matrix

The user deferred Windows/NVDA and Linux/Orca manual verification to
[ticket 250](../tickets/250-windows-linux-screen-reader-verification.md) on
2026-09-13. These rows are unverified follow-up work, not acceptance blockers for
043. The macOS row remains part of 043.

Record OS, application revision, screen-reader version, date, language, input
method and observed result for every run. Execute in both German and English:

| Platform | Native surface | Screen reader | Status |
| --- | --- | --- | --- |
| macOS | Tauri WebView and minimal native app menu | VoiceOver | Partial: settings DE/EN output verified; remaining smoke script pending |
| Windows | Tauri WebView | NVDA | Deferred by user to ticket 250; unverified |
| Linux | Tauri WebView | Orca | Deferred by user to ticket 250; unverified |

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
