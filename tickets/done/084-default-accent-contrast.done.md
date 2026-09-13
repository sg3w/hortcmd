# TICKET-084: Separate accent foreground and surface colors

## Evidence

Browser inspection during ticket 043 found selected settings buttons using
12 px white text (`#ffffff`) on the dark theme's accent (`#3b82f6`). The WCAG
relative-luminance contrast ratio is 3.6779:1, below 4.5:1 for normal text.
The same accent is used as foreground and focus color on the panel (`#2b2d31`),
where it measures 3.7505:1. That is sufficient for non-text indicators but below
4.5:1 when used for small text.

Changing this single accent to the light theme's `#2563eb` gives white text
5.1686:1 but only 2.6688:1 against the dark panel, reducing indicator contrast.
Use separate semantic colors instead of moving the shared color back and forth.

## Requirements

- Separate accent surfaces, text on those surfaces, foreground text and focus
  indicators where their contrast constraints differ.
- Check both default themes, selected/hover states and actual adjacent surfaces.
- Keep custom-theme behavior intentional and document its validation limits.
- Inspect rendered styles after the change and add a regression check for the
  default token pairs; do not claim arbitrary user colors are conformant.
- Run `npm run check`, commit the isolated ticket locally and do not push.

## Scheduling

Interpose before closing ticket 043: its default-contrast requirement cannot be
accepted while the measured failure remains.

## Completed implementation and verification

Tailwind accent text and backgrounds now use separate semantic tokens. Dark text
uses `#bfdbfe`, light text uses `#1d4ed8`, and both filled surfaces use `#1d4ed8`.
The original accent remains available for borders and indicators. Browser
inspection confirmed the new filled color in both themes after restarting Vite
to load its changed Tailwind configuration. White text now measures 6.7016:1.

Two tests read the default CSS values and check normal accent text against panel,
inactive panel, header and selected surfaces, white text at normal/110% brightness,
and indicator contrast against panel surfaces. These checks cover default tokens;
arbitrary user file-color overrides are not certified by them. No user accent
override is currently exposed by the settings registry.

Full `npm run check` passed: 387 Rust tests (25 ignored), 459 frontend tests, all
formatting, Clippy, bindings, typechecking, documentation/UI-text, build and budget
gates. Local commit only; no push.
