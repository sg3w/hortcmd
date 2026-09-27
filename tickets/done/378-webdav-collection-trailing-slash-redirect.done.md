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

## Implementation

`src-tauri/src/vfs/webdav.rs`. Automatic redirect following stays off.

- `collection()` appends the WebDAV collection delimiter, and every request
  whose target is a collection by definition is now addressed with it:
  `PROPFIND` with `Depth: 1` (`entries_of`), `MKCOL` (`create_dir_all`) and
  `DELETE` of a directory (`remove_dir_all`).
- `slash_redirect()` is the predicate for a `3xx` that may be retried: the
  `Location` is resolved against the request URL (so a relative one works) and
  accepted only when scheme, host and port are identical, there is no query or
  fragment, and the target path is the request path plus one trailing slash.
  Everything else — another host, a scheme downgrade, a port change, an
  unrelated path — returns `None`, so no credential is replayed to another
  origin.
- `WebdavProvider::send()` performs a request, and on a redirection retries it
  exactly once against that path. The closure is called with the request path,
  so the retry rebuilds the whole request, headers and body included. A
  redirect that is not resolvable, and a second redirect on the retry, become
  `net.protocol` (`redirect_error`, op `webdav-redirect`) instead of surfacing
  as `internal.unexpected`.
- `Redirect::Follow` / `Redirect::Refuse` decides per operation. `remove_file`
  refuses: a redirect to the collection form would mean the target is a
  collection, where `DELETE` removes a whole tree. `PROPFIND` with `Depth: 0`
  (`stat_entity`, `health_check`) and `MOVE` follow, because either kind of
  resource can be addressed there; the `MOVE` retry slashes the `Destination`
  too, since a redirected source names a collection.
- `PROPFIND` is driven from the raw response (`propfind`/`propfind_async`),
  because `reqwest_dav::Client::list` collapses a redirect into
  `StatusMismatched` and leaves no way to read the `Location`. The multistatus
  body is parsed with the crate's own `ListMultiStatus`/`ListEntity`, so the
  entry mapping is unchanged.

### Verification

- New unit tests: `collection_paths_end_in_one_slash`,
  `only_a_trailing_slash_on_the_same_origin_is_followed` (the real `mod_dav`
  redirect absolute and relative, a cross-host `Location`, a scheme downgrade,
  a port change, an unrelated path, a sub-path, a query, an empty header, and
  an encoded name) and `an_unresolvable_redirect_is_a_protocol_error`. The
  existing PROPFIND server test now asserts `PROPFIND /root/`.
- `npm run test:harness` on macOS (arm64): all six WebDAV tests named above
  pass for both `webdav` and `webdavs` — `harness_listing_matches_the_seeded_tree`,
  `harness_stat_agrees_with_the_listing`, `harness_creates_renames_and_deletes`,
  `harness_stages_and_commits`, `harness_streams_large_content_unchanged`,
  `harness_copies_within_one_server` and `harness_copies_between_different_servers`.
  19 of 22 tests pass; the three that remain are ticket 379's two and
  `harness_copies_between_local_and_remote`, which fails identically for all
  seven services with "remote path at native boundary" — a separate defect,
  not a WebDAV one.
- `npm run check` passed on macOS (arm64).
