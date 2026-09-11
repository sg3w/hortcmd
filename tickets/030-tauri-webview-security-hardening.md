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

- [ ] The production configuration no longer has `csp: null`.
- [ ] The packaged application starts and all Tauri IPC calls still work.
- [ ] Text, Markdown, highlighted code, CSV, image, and EXIF previews still render.
- [ ] Inline scripts, event-handler attributes, `javascript:` links, remote scripts,
      and remote frames are blocked.
- [ ] Clicking an HTTPS Markdown link opens the system browser without navigating
      the hortcmd WebView.
- [ ] Unsupported or malformed link schemes do nothing and produce no IPC call.
- [ ] Direct attempts to navigate the main WebView away from the trusted app origin
      are rejected.
- [ ] Development mode still supports Vite HMR without weakening production CSP.
- [ ] The remaining Tauri capabilities are each required and documented.

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
