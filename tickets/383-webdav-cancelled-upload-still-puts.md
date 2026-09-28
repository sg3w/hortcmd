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
