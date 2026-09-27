# TICKET-378: Follow a WebDAV collection's trailing-slash redirect

Priority: **P1**. Status: **open**.
Discovered while verifying ticket 370 against the protocol harness (2026-09-27).

## Problem

`src-tauri/src/vfs/webdav.rs::http_agent` builds the reqwest client with
`redirect::Policy::none()` (introduced with the transport-security work in
ticket 047, deliberately, so a redirect cannot move a request to another host).
Nothing else compensates for the redirect, so any request whose target is a
collection addressed *without* a trailing slash fails.

Apache `mod_dav` — the reference server the harness uses, and the common case in
the field — answers `PROPFIND /work`, `MKCOL /work`, `DELETE /work/x` and
`PROPFIND /tree/.hidden-dir` for an existing collection with `301` and a
`Location` pointing at the same path plus `/`. hortcmd surfaces that as
`internal.unexpected … StatusMismatched { response_code: 301, expected_code: 207 }`
or, for `MKCOL`, as "Ordner konnte nicht angelegt werden".

Evidence from the harness logs in `harness/.run/`: the run of 2026-09-13
(`4478f5-logs/webdav.log`, before ticket 047) shows the client following the
redirect — `PROPFIND /work` → `301`, then `PROPFIND /work/` → `207` with a
`Referer` of the first URL. The runs of 2026-09-25 and 2026-09-27
(`69ada1-logs`, `5e42f1-logs`) show `MKCOL /work` → `301` and no retry.

This is not harness-only: listing, stat, mkdir, delete, staging and every
transfer into a WebDAV target break against a server that redirects
collections. Six harness tests fail for `webdav` and `webdavs` because of it
(`harness_listing_matches_the_seeded_tree`, `harness_stat_agrees_with_the_listing`,
`harness_creates_renames_and_deletes`, `harness_stages_and_commits`,
`harness_streams_large_content_unchanged`, and the three transfer tests).

## Required change

Keep automatic redirect following disabled. Instead address collections
correctly and handle the redirect explicitly:

- request a collection with a trailing slash where the operation's target is
  known to be a collection (`PROPFIND` with `Depth: 1`, `MKCOL`, `DELETE` and
  `MOVE` of a directory);
- when a `3xx` still arrives, retry once against the `Location` only if it is
  the same scheme, host and port and differs from the request path solely by a
  trailing slash — anything else stays an error, and no credential is replayed
  to a different origin;
- map a redirect that is not resolvable this way to a structured error rather
  than `internal.unexpected`.

## Verification

- Unit coverage for the same-origin/trailing-slash-only predicate, including a
  cross-host `Location`, a scheme downgrade, a port change and an unrelated
  path.
- `npm run test:harness`: the six WebDAV tests above pass for both `webdav` and
  `webdavs`.
- `npm run check`.
