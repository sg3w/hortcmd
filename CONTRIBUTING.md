# Contributing

This is a small repository with a strict habit: **one ticket, one commit, one
green `npm run check`.** Everything below follows from that.

## The one command

```bash
npm run check
```

It is the whole gate and exactly what CI runs, in the same order — so a red
build is always reproducible locally. See
[README § Tests and CI](README.md#tests-and-ci) for what each step does and how
to run them one at a time.

## Tickets

Work is described in `tickets/` before it is done.

- **Open** work is `tickets/NNN-short-slug.md`.
- **Finished** work moves to `tickets/done/NNN-short-slug.done.md` — same
  number, same slug, `.done.md` suffix. Use `git mv` so the history follows.
- Before moving it, append two sections to the file:
  - **`## Implementation`** — what was built and, more importantly, *why it is
    shaped that way*. Anything surprising, any bug the work uncovered, any
    decision a reviewer would otherwise have to reverse-engineer.
  - **`### Verified`** — what was actually run and observed. Say plainly what
    was *not* verified; "not done" is a useful sentence and an untested claim is
    worse than an admitted gap.
- Tick the acceptance criteria only for what is really true.

`tickets/` is tracked. The `.gitignore` carries a commented-out `#tickets/`
line from an earlier decision; leave it commented — the tickets are part of the
record and the `.done.md` files are where the reasoning lives.

## Commits

One commit per ticket, containing only that ticket's changes. Write the message
for someone reading it in a year: what was wrong, what changed, and why that
shape. Do not push unless asked.

## Validation

Formatting and lint are not negotiable and not silenced:

- `cargo fmt` — run it; `check:fmt` fails otherwise.
- Clippy runs over **all targets and features** with `-D warnings`. Fix the
  finding rather than allowing it. A narrow `#[allow]` is acceptable only when
  the API shape is externally required or the alternative is less readable, and
  it carries the reason at the item.
- Tests live beside the module they cover. `src/test/` holds only shared
  machinery.

**Touching a remote provider?** Run the protocol harness before and after:

```bash
npm run test:harness
```

It builds its own SFTP, FTP, explicit and implicit FTPS, SMB and WebDAV
servers, runs the suite against all of them, and deletes everything again.
Docker is the only prerequisite. A provider change that is not exercised there
is a provider change nothing checks — the harness exists because four
data-corrupting bugs lived happily under a green unit-test suite (see
[Concept.md § The protocol harness](Concept.md)). If a protocol genuinely
cannot keep the shared contract, record the difference by name in
`src-tauri/src/vfs/harness/` rather than excusing it in an assertion.

## What must be updated with the code

These have documentation that is part of the contract, and a change to one
is not finished until its documentation matches. `npm run check:docs` enforces
what it can; the rest is on the author.

| If you change … | Update |
|---|---|
| A Tauri command, an event name, or a serialized payload | The IPC table in [Concept.md § 7](Concept.md), and regenerate the bindings (`npm run gen:types`) |
| A provider capability (`src-tauri/src/vfs/caps.rs`) | The provider matrix in [Concept.md § Location capabilities](Concept.md) — checked automatically |
| An error code (`src-tauri/src/errors/`) | `src/i18n/namespaces/errors.ts` in **both** languages, and `npm run gen:errorcodes` — checked automatically |
| A persisted schema (settings, session, selection files, connection profiles) | The description in Concept.md, and a migration path if the old shape is still out there |
| A security policy (CSP, navigation, host keys, credential storage) | [README § Security](README.md#security) — and say what is *not* protected |
| A remote provider (`src-tauri/src/vfs/`) | Run `npm run test:harness`, and extend `src-tauri/src/vfs/harness/` if the change is one the suite would not have caught |

## Security-sensitive changes

Anything touching the CSP, the navigation policy, credential storage, host-key
verification, the capability contract, or error redaction:

- Say in the ticket what the change does to the trust boundary, including what
  it does **not** protect against.
- Never widen the production CSP to make something work; find the way that does
  not need it.
- Never put a secret, a credential-bearing URL, or a raw provider message into a
  user-visible string. Errors carry a code and safe parameters; technical detail
  is redacted and not displayed (see
  [Concept.md § The error boundary](Concept.md)).
- Protocol and keychain tests are `#[ignore]`d and never run in pull-request
  CI. Do not add a test that needs real credentials to the default suite. The
  protocol harness needs none — it generates its own, per run, and deletes them
  with the run; keep it that way.

## Documentation

- **`README.md`** — what the application is, how to set it up, how to run the
  checks, and what the security boundaries actually are.
- **`Concept.md` §1–§11** — the design as it stands. Statements here are
  checked against the code.
- **`Concept.md` §12** — the chronological record. Paths there are paths as they
  were; it is deliberately exempt from the path check.
- **`tickets/done/`** — why each change was made.

Keep the first three true. The fourth is history: correct it only when it is
wrong about what happened.
