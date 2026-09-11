# TICKET-030: Harden the Tauri WebView security boundary

## Goal

Reduce the impact of frontend content injection by enforcing a restrictive
Content Security Policy, preventing untrusted WebView navigation, and handling
links and rich previews without exposing filesystem IPC capabilities to external
content.

## Background

`src-tauri/tauri.conf.json` currently sets `app.security.csp` to `null`. The app
also renders user-controlled Markdown and syntax-highlighted file content with
`dangerouslySetInnerHTML`. Those paths use sanitization or escaping today, but a
file manager's WebView can invoke powerful Rust commands that accept arbitrary
paths. Defense in depth is therefore important: one future rendering mistake
must not become unrestricted filesystem access.

Markdown may contain links to external sites. External content must never replace
the trusted application document inside the main WebView. The security policy
must remain compatible with Tauri IPC, local assets, data-URL image previews,
Radix/Tailwind styling, and the Vite development server.

## Requirements

### Content Security Policy

- Define an explicit production CSP in the Tauri configuration. Start from
  `default-src 'self'` and allow only directives required by measured application
  behavior.
- Permit Tauri IPC and asset protocols using the syntax recommended for the
  installed Tauri v2 version. Do not add broad `http:`, `https:`, `*`, or
  `unsafe-eval` allowances.
- Allow `data:` only where required for generated image previews. Do not allow
  data-URL scripts, frames, or navigations.
- If inline styles are required by React/Radix or dynamic dialog sizing, constrain
  the exception to `style-src`; do not use an inline-style need to weaken scripts.
- Configure a separate development CSP only for the local Vite origin and HMR
  resources that are actually required. Production must not inherit development
  allowances.
- Document every non-default directive and the feature that requires it.

### Navigation and external links

- Add a Tauri navigation policy that allows only the packaged application origin
  and the configured local development origin in development builds.
- Reject top-level navigation to `http`, `https`, `file`, `javascript`, `data`,
  custom protocols, and malformed URLs unless they are the trusted application
  origin itself.
- Markdown links must not navigate the main WebView. Intercept them and open only
  validated `http` or `https` URLs with the operating system's default browser.
- Introduce a narrowly scoped backend command for external URLs or a suitable
  Tauri capability. Validate the parsed scheme; do not reuse an arbitrary path or
  program-launch command as a general URL escape hatch.
- Add `rel="noopener noreferrer"` and safe target behavior where anchors remain in
  rendered markup. Strip unsupported schemes during sanitization.

### Rich-content audit

- Keep Markdown sanitization after rendering. Configure DOMPurify explicitly for
  the elements and attributes the preview needs rather than relying solely on its
  defaults.
- Confirm syntax highlighting escapes source content before returning markup.
- Audit image `data_url` creation, CSV rendering, EXIF values, filenames, terminal
  output, dialogs, and demo-generated SVG for injection or navigation surfaces.
- Do not enable live HTML preview as part of this ticket.
- Add a short security comment at every intentional
  `dangerouslySetInnerHTML` use describing the sanitizer/escaper contract.

### Tauri surface

- Review registered commands and window capabilities for unnecessary exposure.
  Remove unused permissions without breaking required window-state behavior.
- Ensure production developer tools are not enabled by this change.
- Log blocked navigation attempts in development and avoid revealing secrets or
  full file contents in logs.

## Acceptance criteria

- [x] The production configuration no longer has `csp: null`.
- [x] The packaged application starts and all Tauri IPC calls still work.
- [x] Text, Markdown, highlighted code, CSV, image, and EXIF previews still render.
- [x] Inline scripts, event-handler attributes, `javascript:` links, remote scripts,
      and remote frames are blocked.
- [x] Clicking an HTTPS Markdown link opens the system browser without navigating
      the hortcmd WebView.
- [x] Unsupported or malformed link schemes do nothing and produce no IPC call.
- [x] Direct attempts to navigate the main WebView away from the trusted app origin
      are rejected.
- [x] Development mode still supports Vite HMR without weakening production CSP.
- [x] The remaining Tauri capabilities are each required and documented.

## Tests

- Add sanitizer tests for `<script>`, inline event handlers, `javascript:` and
  `data:` links, SVG payloads, encoded schemes, iframes, and nested malformed HTML.
- Test syntax-highlighted source containing closing tags and handler attributes.
- Test the external-link validator with HTTP, HTTPS, file URLs, custom schemes,
  credentials in URLs, control characters, and invalid input.
- Add an application-level test that tries top-level navigation to a local file and
  an external HTTPS page and verifies that both remain outside the WebView.
- Manually verify production and development builds on at least one platform.
- Regression-test all preview modes and the integrated SSH terminal.

## Definition of Done

- The production CSP and navigation policy are active and documented.
- Security tests pass and demonstrate that hostile preview content cannot execute
  scripts, invoke commands, or navigate the trusted WebView.
- Required external links open through a validated system-browser path.
- TypeScript, Rust, and Tauri production builds pass.
- Concept.md records the WebView trust boundary.
- Rename this file to `030-tauri-webview-security-hardening.done.md`.

---

## Implementation

### Content Security Policy (`src-tauri/tauri.conf.json`)

`csp` is no longer `null`. Production starts from `default-src 'self'`; every
non-default directive and why it is there:

| Directive | Value | Required by |
|---|---|---|
| `script-src` | `'self'` | App bundle. Tauri injects its own **nonce** for the IPC bootstrap (`manager::replace_csp_nonce`), so no inline allowance is needed. |
| `style-src` | `'self' 'unsafe-inline'` | React/Radix inline styles and the dynamic dialog sizing. Deliberately confined here — scripts are **not** widened. |
| `img-src` | `'self' data:` | `data:` only for the base64 image previews `read_preview` returns. |
| `connect-src` | `'self' ipc: http://ipc.localhost` | The Tauri IPC transport. |
| `media-/object-/child-/frame-/worker-/manifest-src` | `'none'` | Nothing uses them; closing them removes whole classes of surface. |
| `base-uri` | `'self'` | Stops injected `<base>` from re-pointing relative URLs. |
| `form-action` | `'none'` | Nothing submits forms. |
| `frame-ancestors` | `'none'` | The app is never framed. |

No `http:`, `https:`, `*` or `unsafe-eval` anywhere. `devCsp` is a separate key
covering only the Vite origin, its HMR WebSocket and React Refresh's inline
scripts — production inherits nothing from it.

### Navigation policy (`src-tauri/src/lib.rs`)

`is_trusted_origin` accepts the app document only (`tauri:` /
`http://tauri.localhost`, plus `http://localhost:1420` behind
`cfg!(debug_assertions)`), so `http(s)`, `file:`, `javascript:`, `data:`,
foreign custom protocols and malformed URLs are all refused. The handler can
only be attached while the WebView is created, so the window is declared with
`"create": false` and built in `setup` via `WebviewWindowBuilder::from_config` —
it keeps every property from the config. Blocked attempts are logged in
development with the **URL alone**.

### External links

`open_external` (`commands/fs/preview.rs`) is the only way out of the WebView.
`is_safe_external_url` requires an absolute `http(s)` URL with a host and
rejects control characters and whitespace as a whole-string check — a naive
`starts_with` would pass `\njavascript:…` — plus credentials in the authority.
`open_path` keeps its role as the *file* entry point and is never reused for
URLs. In the frontend, a click inside rendered Markdown is **always**
swallowed; only `isExternalLink` hrefs produce an IPC call at all.

### Sanitization and audit

DOMPurify is configured explicitly: an allowlist of the elements Markdown
produces, an attribute list without `style`/`on*`/`target`, and
`ALLOWED_URI_REGEXP` accepting only absolute `http(s)` and `#fragment`.
Relative hrefs are stripped deliberately — they are same-origin and would pass
the navigation policy, replacing the running app. An `afterSanitizeAttributes`
hook drops `target` and sets `rel="noopener noreferrer"`. Both
`dangerouslySetInnerHTML` sites carry the contract they rely on as a comment.

Audited and found safe without change: CSV cells, EXIF values, filenames,
dialog text (all React text children, escaped by React) and terminal output
(xterm renders bytes, never `innerHTML`). Changed: the demo image placeholder
interpolated a filename into SVG markup and now escapes it. `escapeHtml` also
escapes quotes. Live HTML preview stays unimplemented, as the ticket requires.

### Capabilities

`core:window:allow-set-focus` was unused (the only `focus()` calls are DOM
ones) and is removed. The remaining entries are each in use and the capability
file now names the caller of each.

## Verified

- `cargo test --lib`: **201 passed**, 6 new for `is_safe_external_url` /
  `open_external` — http/https with ports and IPv4, every other scheme,
  malformed input, whitespace and control characters, credentials in the URL,
  and that the command refuses before launching anything.
- `npm run test` (**vitest added**, 19 tests in `src/lib/preview.test.ts`, run
  against jsdom because DOMPurify works on nodes): scripts, inline event
  handlers, `javascript:`/`data:` links, relative links, SVG payloads,
  iframes/objects/embeds, styles, forms, encoded and obfuscated schemes,
  nested malformed HTML, plus highlight.js escaping closing tags and handler
  attributes — and that ordinary Markdown still renders.
- **Application level, on macOS.** The debug (dev) build and a
  `--features custom-protocol` build (which is what turns off Tauri's `dev`
  cfg, so it serves `../dist` under the **production** CSP) were both launched
  and instrumented temporarily:
  - dev build: `nav allowed=true url=http://localhost:1420/` followed by
    `list_drives` reaching the backend — window creation, devCsp and IPC all
    work.
  - production-CSP build: the app booted from `dist` and `list_drives` reached
    the backend, confirming `script-src 'self'` is satisfied by Tauri's nonce.
  - navigation attempts from inside the page: `window.location.href` and
    `window.open(…, "_self")` to `https://example.com/` both logged
    `nav allowed=false` and the app kept running on its own document;
    `file:///etc/passwd` never even reached the handler. The instrumentation
    was removed afterwards (`git status` clean of it).
- `npm run typecheck` and `npm run build` pass.

**Not covered:** verification ran on macOS only — Windows and Linux resolve the
app origin as `http://tauri.localhost`, which the policy accepts, but that was
not exercised on a machine. The preview modes and the SSH terminal were not
re-run end to end against a live server; the terminal touches no HTML sink
(xterm writes bytes) and the preview modes are covered by the sanitizer tests
plus the production-CSP boot. vitest is pinned to `^2` because the project is
on Vite 5; the current vitest needs Vite 6+.
