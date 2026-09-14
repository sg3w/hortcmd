# Native file icons (TICKET-065)

File view defaults to **System icons**, with a validated, persisted **Simple icons**
choice. Existing installations without the field get system icons. German and
English explain the initial delay. File-table details, list and thumbnail modes,
the directory tree and file picker use the same RowIcon component. Toolbar action
icons, favorite buttons and the parent entry remain action/navigation symbols.

macOS uses the already locked objc2 0.6.4 / objc2-app-kit 0.3.2 bindings, NSWorkspace
and an offscreen NSBitmapImageRep. Apple's documentation explicitly permits
[iconForFile](https://developer.apple.com/documentation/appkit/nsworkspace/icon(forfile:))
and [iconForFileType](https://developer.apple.com/documentation/appkit/nsworkspace/icon(forfiletype:))
on background threads. The latter preserves compatibility with the app's older
macOS deployment range; iconForContentType requires macOS 11. A single worker owns
its graphics context and autorelease pool; shared NSImages are never resized.
Windows/Linux adapters are deliberately deferred to TICKET-265, as requested.
They currently return the normal simple fallback.

Local requests carry the displayed entry's full identity, including modification
time. We conservatively cache local files by path as well as directories, retaining
per-file associations and custom Finder icons. Remote/archive requests carry only
a filename/type, never a provider address. The Rust adapter refuses nonabsolute or
parent-traversing paths, avoids all cached network mounts (including hidden mounts),
and uses type icons for links or paths with symlink ancestors. It never opens a VFS
provider, downloads content, extracts archives or generates content thumbnails.
A disappearing/unreadable local entry uses its type icon or the simple fallback.

Links use their listing metadata and a single accessible bottom-left arrow, in both
modes and all sizes. The native base for a link is always a type icon, avoiding a
second native arrow and any lookup of the target. Ordinary icons are decorative.
PNG images keep their own colors independently of Git and selection text colors;
a fixed-size wrapper prevents layout shifts.

## Limits and invalidation

- Only intersecting rendered rows, plus an 80px margin, subscribe. The existing
  virtualizers bound mounted rows; the picker also uses intersection visibility.
- One batch runs at a time, 16 icons per frontend batch. At most 512 identities wait;
  queued entries lose their work when their last row unsubscribes. Concurrent
  subscriptions share one result. Requests above the queue cap retain simple icons
  and retry on the periodic check.
- Rust accepts at most 32 entries per call and one running batch, using a semaphore
  acquired before spawning. Excess calls fall back immediately rather than growing
  a background queue. PNGs are 12–128 physical pixels, at most 96 KiB each; there
  are no new asset endpoints. The frontend also rejects non-PNG/oversized responses.
- The LRU holds at most 256 entries and 4 MiB of encoded image/key strings (counted
  conservatively as two bytes per character). Native bitmap allocation is at most
  64 KiB per image, excluding OS-owned caches. No object URLs or persisted images.
- Positive TTL: 60 seconds; negative TTL: 5 seconds. Visible rows recheck every
  five seconds. Path/mtime, resolved theme and physical size change the identity;
  resize/focus and theme observation re-evaluate it. Association/custom-icon changes
  that do not alter mtime are refreshed within 65 seconds. Mode switches detach
  pending consumers immediately; simple mode submits no native calls.
- Late results can populate only their original cache identity. Unsubscribed rows
  cannot be updated; each rendered image must also match the current identity.

## Verification — macOS, 2026-09-14

Automated coverage includes request/path/mtime/theme/scale identities, remote,
archive and link type-only requests, simple/parent overlays, late navigation and
mode changes, deduplication during queued/running work, queue/concurrency caps,
LRU entry/byte limits, positive/negative expiry and transport failures. Settings
migration tests cover absent, valid and invalid mode values. The full gate is
`npm run check`.

Native integration tests are opt-in (ordinary CI needs no Workspace session):

```sh
cargo test --manifest-path src-tauri/Cargo.toml native_png_sizes_and_timing -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml native_custom_and_benchmark -- --ignored --nocapture
```

The native tests verified exact PNG widths 14, 28, 40, 80 and 128; five first-lookups
completed in 992.7 ms. A temporary file received TextEdit's custom Finder icon;
its returned PNG changed and was visually inspected at 80px. Owned temporary files
were removed by the test. 96 local icons across six extensions (including unknown)
took **3.209 s cold / 23.50 ms warm in the OS cache**, with **148,480 encoded transport
bytes** per pass. These timings include fixture file writes and are not promises
about arbitrary disks or file association handlers.

A temporary browser fixture exercised 5,000 entries in each pane with the real
queue, real native PNG output, and a deliberately fixed 40ms simulated IPC response.
The native worker and browser were measured separately, not as an end-to-end Tauri
benchmark. All fixture changes were removed afterwards.

| Browser development measurement | Simple | System, cold frontend cache |
| --- | ---: | ---: |
| First populated list | 342 ms | 266.9 ms |
| First PNG after page start | none | 348.1 ms |
| Cached identities at rest | 0 | 47 |
| Retained encoded image/key strings | 0 | 720,660 bytes |
| Running requests after settling | 0 | 0 |

One dev-build sample per mode; the difference in listing time is startup noise,
not evidence that system icons accelerate listing. Maximum initial timer lag was
315 / 240ms respectively (includes initial 5,000-row preparation); End/Home and
view/mode switches stayed operable during subsequent loading. Returning to cached
rows reused results. The image count stayed around the viewport (51), rather than
resolving 10,000 files. After changing theme, separate cache identities occupied
1,441,414 bytes. The test images intentionally all used one known native PNG to
isolate queue/layout behavior from OS lookup variability.

Manual browser inspection covered DE/EN settings, dark/light themes, persisted
simple/system switching, details/thumbnails, selection and symlink overlays while
scrolling and reloading. Native application automation selected an older installed
window, so no end-to-end native GUI measurement is claimed. TICKET-266 tracks that
remaining platform/GUI verification, including list/tree/picker and multi-monitor
scale transitions. Backend native integration and common UI regression tests passed.
