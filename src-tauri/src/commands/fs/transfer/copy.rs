use crate::vfs::StreamPath;
// ============================================================
// The sequential copy path (ticket 035).
//
// One thread, one file at a time: read a block, write it, report, honour
// pause and cancel, and — with verification on — compare checksums at
// the end. `copy_tree` walks a directory the same way.
//
// `transfer_one_with` holds the ordering that makes a move safe: write
// the bytes, *then* commit, and only then remove the source. It takes
// the copy as a parameter precisely so a test can fail each step in
// turn and check that nothing is lost.
//
// What a transfer promises about metadata (ticket 385)
// ----------------------------------------------------
// A same-store `rename` — the fast path of a move within one device —
// keeps the object, so its timestamps, mode and extended attributes are
// untouched by definition. Everything else is a streamed copy, which
// reproduces only what both ends can name: `preserve_metadata` applies the
// source's modification time and, on Unix, its permission bits, to files and
// to directories. Extended attributes, ACLs, ownership and creation times are
// *not* carried, on any path.
//
// A move across devices is therefore a copy: it keeps mtime and mode, and
// loses the rest, rather than silently differing from the rename it falls back
// from. A cross-provider copy keeps whatever the source can report and the
// destination can apply — today that is nothing, because no remote adapter
// implements either half, so such a plan keeps its `metadataUnknown` advisory.
// ============================================================

use super::events::ItemError;
use super::plan::{discard_staging, publish, Plan};
use super::progress::Prog;
use crate::errors::{AppError, ErrorAction};
use crate::vfs::Provider;
use std::io::{ErrorKind, Read, Write};
use std::time::{Duration, Instant};

/// The three knobs every copy shares, from the settings down to the
/// innermost block loop. Carried as one value so the recursive and
/// parallel paths do not thread three unrelated parameters through
/// every level — which is what pushed several of these functions past
/// a readable signature.
#[derive(Clone, Copy)]
pub(crate) struct CopyTuning {
    /// Speed cap in bytes per second; 0 = unlimited.
    pub(crate) limit: u64,
    /// Compare the SHA-256 of source and target after copying.
    pub(crate) verify: bool,
    /// Block size of the copy loop.
    pub(crate) buf_size: usize,
}

/// Copies a file in blocks; reacts to cancel (the partial file is
/// removed) and pause.
pub(crate) fn copy_file_prog(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    src: &StreamPath,
    dst: &StreamPath,
    prog: &mut Prog,
    tune: CopyTuning,
) -> std::io::Result<()> {
    let CopyTuning {
        limit,
        verify,
        buf_size,
    } = tune;
    use sha2::{Digest, Sha256};

    let file_total = srcp.stat(src)?.len;
    let name = src.to_string_lossy().into_owned();
    let mut reader = srcp.open_read(src)?;
    let mut writer = dstp.create_write(dst)?;
    let mut buf = vec![0u8; buf_size];
    let mut file_done = 0u64;
    let mut hasher = verify.then(Sha256::new);

    // Rate-limit window (only for limit > 0): reset roughly every 1 s.
    let mut win_start = Instant::now();
    let mut win_bytes = 0u64;

    let abort = |writer: Box<dyn Write + Send>, dst: &StreamPath| {
        drop(writer);
        let _ = dstp.remove_file(dst); // unvollständige Zieldatei verwerfen
        std::io::Error::new(ErrorKind::Interrupted, "abgebrochen")
    };

    prog.emit(&name, 0, file_total, true);
    loop {
        if prog.cancelled() {
            return Err(abort(writer, dst));
        }
        // Block while paused; afterwards check for cancel again if needed.
        prog.wait_if_paused(&name, file_done, file_total);
        if prog.cancelled() {
            return Err(abort(writer, dst));
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        super::recovery::fault("write");
        if let Some(h) = hasher.as_mut() {
            h.update(&buf[..n]);
        }
        file_done += n as u64;
        prog.bytes_done += n as u64;
        prog.emit(&name, file_done, file_total, false);

        // Speed limit: sleep briefly when exceeded.
        if limit > 0 {
            win_bytes += n as u64;
            let elapsed = win_start.elapsed().as_secs_f64();
            let allowed = limit as f64 * elapsed;
            if win_bytes as f64 > allowed {
                let wait = (win_bytes as f64 - allowed) / limit as f64;
                std::thread::sleep(Duration::from_secs_f64(wait.min(1.0)));
            }
            if elapsed >= 1.0 {
                win_start = Instant::now();
                win_bytes = 0;
            }
        }
    }

    // Finalize remote buffered writers even when checksum verification is off.
    writer.flush()?;
    super::recovery::fault("flush");

    // Verification: read the target again and compare checksums.
    if let Some(h) = hasher {
        // Flush first. A local `File` is already on disk by now, but a
        // remote writer (SFTP/FTP/WebDAV) may still be holding the tail
        // of the file — verifying before that lands would compare
        // against an incomplete copy and report a corruption that is
        // not there.
        writer.flush()?;
        super::recovery::fault("flush");
        let src_hash = h.finalize();
        let dst_hash = hash_file_sha256(dstp, dst)?;
        if src_hash.as_slice() != dst_hash.as_slice() {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Prüfsumme stimmt nicht (Kopie beschädigt)",
            ));
        }
    }

    // The bytes are committed; the fidelity is reported, never fatal.
    if let Err(error) = preserve_metadata(srcp, dstp, src, dst) {
        super::progress::note(&prog.notes, &name, metadata_error(error));
    }

    prog.files_done += 1;
    prog.emit(&name, file_total, file_total, true);
    Ok(())
}

/// Reproduces the source's timestamp and mode on the entry just written.
///
/// Applied to the name the bytes were written under — in safe mode that is the
/// staging name, and the publishing rename carries the metadata with it. Both
/// halves are optional: a source that reports nothing and a destination that
/// cannot apply anything both leave the target exactly as written, which is
/// what the plan's `metadataUnknown` advisory tells the user beforehand
/// (ticket 385).
pub(crate) fn preserve_metadata(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    src: &StreamPath,
    dst: &StreamPath,
) -> std::io::Result<()> {
    let Some(metadata) = srcp.read_metadata(src)? else {
        return Ok(());
    };
    if metadata.is_empty() {
        return Ok(());
    }
    match dstp.apply_metadata(dst, &metadata) {
        Err(error) if error.kind() == ErrorKind::Unsupported => Ok(()),
        other => other,
    }
}

/// A failed metadata step is reported against the item, not the operation.
pub(crate) fn metadata_error(error: std::io::Error) -> AppError {
    AppError::from(error)
        .op("preserve-metadata")
        .action(ErrorAction::Report)
}

/// SHA-256 of a file (for the copy verification).
pub(crate) fn hash_file_sha256(
    provider: &dyn Provider,
    path: &StreamPath,
) -> std::io::Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    let mut f = provider.open_read(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_vec())
}

/// Copies recursively (file or directory tree) from `srcp` to `dstp`.
pub(crate) fn copy_tree(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    src: &StreamPath,
    dst: &StreamPath,
    prog: &mut Prog,
    tune: CopyTuning,
) -> std::io::Result<()> {
    if srcp.is_dir(src) {
        dstp.create_dir_all(dst)?;
        for child in srcp.read_dir(src)? {
            let name = child.file_name().unwrap_or_default();
            copy_tree(srcp, dstp, &child, &dst.join(name)?, prog, tune)?;
        }
        // After the children: writing them is what last changed this
        // directory's own timestamp.
        if let Err(error) = preserve_metadata(srcp, dstp, src, dst) {
            super::progress::note(&prog.notes, &src.to_string_lossy(), metadata_error(error));
        }
    } else {
        if let Some(parent) = dst.parent() {
            dstp.create_dir_all(&parent)?;
        }
        copy_file_prog(srcp, dstp, src, dst, prog, tune)?;
    }
    Ok(())
}

/// The order that makes safe mode safe: write the bytes, *then* commit,
/// and only then remove a move's source. `copy` produces the bytes in
/// `plan.write_to`; it is a parameter so this ordering can be tested
/// with a failure injected at each step.
pub(crate) fn transfer_one_with<F>(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    src: &StreamPath,
    plan: &Plan,
    remove_source: bool,
    errors: &mut Vec<ItemError>,
    copy: F,
) -> std::io::Result<()>
where
    F: FnOnce() -> std::io::Result<()>,
{
    // Until this returns, the old destination has not been touched.
    plan.writing()?;
    if let Err(e) = copy() {
        discard_staging(dstp, plan, errors);
        return Err(e);
    }
    if let Err(e) = publish(dstp, plan) {
        discard_staging(dstp, plan, errors);
        return Err(e);
    }
    // The destination is committed. A source that cannot be removed
    // leaves two copies — which is reported, never "repaired" by
    // deleting the copy that just succeeded.
    if remove_source {
        if let Err(e) = srcp.remove(src) {
            // Two copies now exist. Reported as its own failure — never
            // "repaired" by deleting the copy that just succeeded.
            errors.push(ItemError::new(
                src.display().to_string(),
                AppError::from(e).op("move").action(ErrorAction::Report),
            ));
        } else if let Some(step) = &plan.recovery {
            if srcp.store_id() == "local" {
                super::recovery::durability::sync_parent(src.native()?)?;
            }
            step.transition(super::recovery::types::Phase::SourceRemoved)
                .map_err(std::io::Error::other)?;
        }
    }
    Ok(())
}

/// Copies one top-level source and publishes it under its final name.
#[allow(clippy::too_many_arguments)]
pub(crate) fn transfer_one(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    src: &StreamPath,
    plan: &Plan,
    prog: &mut Prog,
    tune: CopyTuning,
    remove_source: bool,
    errors: &mut Vec<ItemError>,
) -> std::io::Result<()> {
    transfer_one_with(srcp, dstp, src, plan, remove_source, errors, || {
        copy_tree(srcp, dstp, src, &plan.write_to, prog, tune)
    })
}

/// Resolves a buffer size in KB into a safe byte count
/// (0 = default 256 KB; otherwise 4 KB … 16 MB).
pub(crate) fn resolve_buf(buf_kb: u64) -> usize {
    if buf_kb == 0 {
        256 * 1024
    } else {
        (buf_kb.clamp(4, 16 * 1024) as usize) * 1024
    }
}
