# TICKET-383: Do not upload the spool of a cancelled WebDAV transfer

Priority: **P1**. Status: **open**.
Discovered during a full code review (2026-09-28).

## Problem

Cancelling a copy to a WebDAV target uploads the whole partial file anyway,
and only then deletes it again.

`WebdavWriter` spools writes to a temp file and sends the `PUT` from
`finish()` (`src-tauri/src/vfs/webdav.rs:764-788`). `Drop` calls `finish()` as
a "safety net for un-flushed streams" (`webdav.rs:710-713`, `804-809`):

```rust
impl Drop for WebdavWriter {
    fn drop(&mut self) {
        let _ = self.finish();          // ← performs the PUT
        let _ = std::fs::remove_file(&self.spool_path);
    }
}
```

The cancel path in the copy loop does exactly the thing that triggers it
(`src-tauri/src/commands/fs/transfer/copy.rs:67-71`, and the identical closure
in `parallel.rs:142-146`):

```rust
let abort = |writer: Box<dyn Write + Send>, dst: &StreamPath| {
    drop(writer);                       // ← WebdavWriter::drop → PUT
    let _ = dstp.remove_file(dst);      // ← then DELETE
    std::io::Error::new(ErrorKind::Interrupted, "abgebrochen")
};
```

So pressing Cancel on a half-finished 4 GB upload sends the 2 GB already
spooled to the server, waits for it, and then issues a `DELETE`. Consequences:

- Cancel does not cancel. The user waits for the rest of the upload with no
  progress reported and no way to interrupt it a second time.
- The truncated object is briefly published under the destination name. In
  direct mode that name *is* the user's target. If the following
  `remove_file` fails — the network is the reason the user cancelled, plausibly
  — a truncated file is left behind under the real name. The error is
  discarded (`let _ =`), so nothing reports it.
- `copy.rs:38-39` documents the opposite contract: "reacts to cancel (the
  partial file is removed)".

`finish()` cannot distinguish the two callers, because `Write` has no
"abandon" operation — `flush()` means commit, `drop` today means the same.

## Required change

- Give `WebdavWriter` an explicit abandon path and make `Drop` use it: on
  drop, discard the spool without a `PUT` unless the stream was committed.
  A dropped, never-flushed stream is an abandoned transfer, not a completed
  one; the transfer engine always calls `flush()` on the success path
  (`copy.rs:113`, `parallel.rs:172`), so nothing legitimate relies on the
  upload happening in `Drop`.
- Check the other writers for the same asymmetry — anything whose `Drop`
  publishes rather than discards.
- Keep the spool file removal in `Drop` (that part is right), and keep
  `flush()` reporting its error to the caller.

## Verification

- Rust test with a local HTTP listener standing in for the DAV server: write
  a few blocks to a `WebdavWriter`, drop it without flushing, assert the
  server received no `PUT` and the spool file is gone.
- Rust test: write, `flush()`, assert the `PUT` arrives with the full body.
- `npm run test:harness`: the WebDAV transfer tests stay green.
- `npm run check`.

## Implementation

`WebdavWriter` now has two exits instead of one.

- `finish()` is unchanged and remains the commit: `flush()` calls it, and its
  error reaches the transfer engine.
- `abandon()` marks the stream finished, closes the spool handle and removes
  the spool file — without a `PUT`. `Drop` calls it.

A writer that is dropped without a `flush` is by definition an abandoned
transfer: the engine flushes on every success path (`copy.rs`, `parallel.rs`),
and the cancel closure drops the writer precisely to throw the partial write
away. Pressing Cancel therefore no longer uploads the spooled prefix, no
truncated object is ever published under the destination name, and the
following `remove_file(dst)` has nothing to clean up on a direct-mode upload
that never started.

### The other writers

Checked, and none has the same asymmetry. `SmbWriter`, `SyncSftpFile` and
`FtpWriter` all put their bytes on the wire as they are written; their `finish`
/ `Drop` only closes the handle (and, for FTP, finalizes the data stream and
reads the reply). A partial file at the destination is already there when
cancel arrives, which is the local-filesystem behaviour the cancel path is
written for — it deletes the destination. WebDAV was the only writer that
withheld the whole body until the end, so it was the only one whose `Drop`
could publish.

### Verification

- `vfs::webdav::tests::a_dropped_upload_sends_nothing_and_a_flushed_one_sends_everything`:
  a `WebdavWriter` pointed at a local `TcpListener` is written to and dropped;
  the listener never receives a connection (non-blocking `accept` answers
  `WouldBlock`) and the spool file is gone. The second half writes 4 KiB,
  flushes, and the stand-in server receives `PUT /kept.bin` with the complete
  body.
- `npm run check` passed on macOS (arm64).
- `npm run test:harness` was not run: the Docker daemon is not available in
  this environment. The change removes a request rather than adding one, and
  the harness's WebDAV transfers all commit through `flush()`.
