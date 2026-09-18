# TICKET-304: Classify SFTP file types by their complete mode discriminant

## Problem

The existing SFTP provider calls the locked russh-sftp 2.4.0 metadata helpers
`is_dir`, `is_regular` and related methods. Those helpers test whether a set of
mode bits *contains* a type constant. POSIX file types are mutually exclusive
values under the `0xf000` mask, not independent flags: a socket (`0xc000`) and
block device (`0x6000`) both contain the directory bit pattern (`0x4000`).
Consequently, SFTP listing/stat can present special files as directories, and
recursive operations can attempt to descend into them. Listing, target metadata
and recursive removal currently use these helpers in `src-tauri/src/vfs/sftp.rs`.
The corrected provider compares the complete masked type value.

## Required change

- Add one exact SFTP file-type classification helper using the full type mask.
- Use it consistently for provider stat/listing, symlink target presentation and
  recursive traversal decisions. Preserve existing symlink semantics.
- Treat missing/unknown type metadata explicitly; never infer a directory from
  partial or overlapping type bits.
- Do not change remote entries or perform destructive live-server tests.

## Tests and completion

Test regular files, directories, links, FIFO, socket, character/block devices,
unknown values and missing mode, including ordinary permission bits. Cover
listing/stat and recursive traversal decisions with fixtures. Run `npm run check`,
commit this ticket on its own branch, and move the file to
`tickets/done/304-sftp-file-type-discriminants.done.md` after completion.

## Completed

One shared metadata classifier now drives provider listing, stat and recursive
directory decisions. Special files and unknown types are never classified as
directories; resolved and broken symlinks retain their established presentation.
Three focused fixture tests cover all mode discriminants, permission combinations,
missing metadata, symlink targets and ordinary entry attributes. The full
`npm run check` passed; entry bundle 317.7 KiB (325 KiB budget).
