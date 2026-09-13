// ============================================================
// Every error code the backend can send (TICKET-036).
//
// Generated from `src-tauri/src/errors/mod.rs` by
// `scripts/gen-error-codes.mjs` and checked in, so the translation test
// can walk the real list rather than a hand-kept copy. `npm run check`
// regenerates it and fails if this file is out of date — the same rule
// the ts-rs bindings follow.
// ============================================================

export const ERROR_CODES = [
  "fs.notFound",
  "fs.permission",
  "fs.exists",
  "fs.notADirectory",
  "fs.isADirectory",
  "fs.readOnly",
  "fs.noSpace",
  "fs.inUse",
  "fs.io",
  "fs.invalidName",
  "fs.tooLarge",
  "transfer.checksum",
  "transfer.noStaging",
  "transfer.commit",
  "transfer.sourceGone",
  "transfer.intoItself",
  "archive.passwordRequired",
  "archive.passwordWrong",
  "archive.unknownFormat",
  "archive.unsafeEntry",
  "archive.limit",
  "archive.conflict",
  "archive.corrupt",
  "net.unreachable",
  "net.timeout",
  "net.auth",
  "net.protocol",
  "net.notConnected",
  "net.lost",
  "net.noProfile",
  "net.unsupportedProtocol",
  "net.noShell",
  "net.tls",
  "cred.locked",
  "cred.wrongPassword",
  "cred.store",
  "cred.notFound",
  "cred.readOnlyFormat",
  "security.hostKeyUnknown",
  "security.hostKeyMismatch",
  "security.blockedUrl",
  "capability.unsupported",
  "capability.offline",
  "validation.badAddress",
  "validation.unknownScheme",
  "validation.badInput",
  "cancelled",
  "internal.unexpected",
] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];
