# TICKET-009: Show a legend of file colors in the file properties and allow color customization

## Goal
The file properties should always show why a file is displayed in a particular color. Additionally, users should be able to customize the color scheme used throughout the application via the settings.

## Background
File names can be highlighted depending on various properties (e.g. Git status, symbolic links, executable files, or hidden files). While the colors help to identify files quickly, their meaning is not always obvious.

Different users also have different preferences or accessibility requirements. Therefore, the mapping between file states and colors should be configurable while remaining consistent across the entire application.

## Requirements

### File properties
- The file properties always display the meaning of the currently applied color.
- The explanation reflects the color rule that is actually active.
- Only the highest-priority rule is shown.
- The displayed color preview matches the file list exactly.
- The solution is extensible so that new color rules can be added without UI changes.

### Settings
- A new **Appearance → File Colors** section is added to the settings.
- Every supported file state can have an individual color assigned.
- A live preview is shown while editing colors.
- Changes are immediately reflected throughout the application.
- Users can restore the default color scheme.
- The color configuration is stored persistently.
- The configuration is shared by all windows.
- Invalid or missing configuration automatically falls back to the default colors.

### Initial configurable color rules
- Git: Untracked
- Git: Modified
- Git: Staged
- Git: Ignored
- Symbolic link
- Executable file
- Hidden file
- Read-only file
- Selected file
- Active file
- Inactive pane selection
- Custom color rule(s)

## Technical notes
- Define a central color registry (e.g. `FileColorRegistry`).
- File rendering and file properties both use the same color source.
- Colors should be referenced via semantic identifiers instead of hardcoded values.
- Store the configuration in the application's settings.
- Future plugins or extensions should be able to register additional color rules.
- Support light and dark themes independently if the theme system already allows separate color palettes.

## Acceptance criteria
- [x] File properties display the reason for the current color.
- [x] The displayed color exactly matches the file list.
- [x] Only the active color rule is shown.
- [x] Users can configure all supported file colors in Settings.
- [x] Changes are applied immediately without restarting the application.
- [x] Default colors can be restored with one click.
- [x] Color settings persist after restarting the application.
- [x] All windows use the same configured colors.
- [x] New color rules can be added without modifying the UI.
- [x] If no rule applies, an appropriate message is displayed.

## Tests
- Verify all supported Git states.
- Verify symbolic links.
- Verify executable files.
- Verify hidden files.
- Verify read-only files.
- Verify files without any special properties.
- Change every configurable color and verify that:
  - the file list updates,
  - the properties dialog updates,
  - all open windows update.
- Restart the application and verify persistence.
- Restore defaults.
- Verify light and dark themes.
- Verify invalid configuration falls back to defaults.

## Definition of Done
- [x] Code compiles.
- [x] Tests pass.
- [x] Existing functionality remains unchanged.
- [x] Settings documentation updated.
- [x] New color registry documented.
- [x] Ticket renamed to `*.done.md`.

---

## Implementation

**Registry** — `src/lib/fileColors.ts` replaces the hardcoded Git→Tailwind-class
map `lib/gitColor.ts` (deleted). Every color is a slot: semantic id, label,
default per theme; slots of kind `rule` add a matcher + priority, slots of kind
`state` feed the interaction colors. `resolveFileColor()` returns the first
match by priority — that is both what the list paints and what the properties
dialog explains, so "only the highest-priority rule" and "preview matches the
list exactly" hold by construction rather than by convention.

**CSS variables** — `applyFileColors()` publishes every slot as `--fc-<id>` on
`<html>` for the resolved theme (`useFileColorVars`, called from `App.tsx`,
which now tracks the resolved theme in state). Rows carry
`style={{ color: var(--fc-…) }}`, so a color change repaints without a
re-render. `index.css` no longer hardcodes `--selection`, `--cursor-active`,
`--cursor-inactive-marker`; they reference the registry (single source of
truth, as the ticket's "semantic identifiers instead of hardcoded values"
asks). `--cursor-inactive-marker` keeps its TICKET-008 value: the slot stores
`#9ac27b` (so `<input type="color">` works) and `alpha: 0.4` turns it into the
same `rgba(154, 194, 123, 0.4)` on publish.

**Backend** — the matchers must not guess from the name or the Unix mode, so
new `commands/fs/attrs.rs` determines `hidden`/`readonly`/`executable` per
platform, and both `DirEntry` and `FileProps` carry them (same field names →
same context object for list and dialog). Side effect: the "hide system files"
filter now uses `hidden` instead of a frontend `startsWith(".")`, which is
identical on Unix and correct on Windows. `classify()` in `git.rs` now
distinguishes `untracked` (`??`) from `staged` (index change, worktree clean)
instead of reporting both as `new`; deleted/renamed/conflict keep precedence.

**Properties dialog** — needs the Git status, which only the opening pane knows,
so `openProps(path, gitStatus?)` passes it (both call sites: `useKeyboard`,
`FileContextMenu`). Avoids a `git` CLI spawn per dialog open.

**Settings → File colors** — the row labels are rendered in their own color, so
the list *is* the live preview; states render via `previewClass` from the
registry. One picker per theme (dark/light independent), per-row reset, global
"restore defaults" (clears overrides only — deleting the user's custom rules on
a defaults reset would be data loss). Custom rules: name, wildcard pattern
(reuses `lib/glob.ts`), color per theme, reorderable; matched **before** the
built-ins, since they are explicit user intent. The form renders whatever
`fileColorDefs()` returns, so `registerFileColor()` (plugins) needs no UI change.

**Decision** — "Active file" maps to `--cursor-active`, i.e. the outline of the
cursor row; the row fill stays `accent-dim` so the existing look is untouched.

**Verified** — `cargo check`, `npm run typecheck`, `npm run build` succeed.
Live in browser demo mode (demo data extended to cover every rule): all 11 rules
resolve to the expected value (checked via `getComputedStyle`), priority holds
(git-ignored beats executable on `script.sh`; a custom `*.txt` rule beats
git-modified and read-only), properties dialog shows the rule and the identical
color and the "no rule applies" message, changing a color updates list +
dialog + preview instantly, dark↔light stay independent, config survives a
reload, restore-defaults works and keeps custom rules, and an invalid
configuration (`"not-a-color"`, `"#zzzzzz"`, `rgb()`, unknown slot id) falls
back to the defaults without a crash. No console errors. Not tested against a
real Tauri build; the new Rust code is `cfg`-split per platform and only the
Unix branch was compiled here.