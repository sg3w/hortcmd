//! Shared ZIP creation: validated inputs, bounded collection, private staging,
//! and publication only after the archive is finished and synced (TICKET-070).
//!
//! An entry the packer cannot take is skipped rather than fatal (TICKET-388).
//! A real tree holds symlinks, sockets, names with a trailing dot, directories
//! this user may not open and files that change while the pack runs; losing the
//! whole archive over one of them is what made the ZIP command unusable. What
//! stays fatal is what a skip would defeat: the archive limits, a destination
//! inside a source, an authorization that says no, and cancellation.
use super::{safety, staging};
use crate::commands::fs::transfer::events::ItemError;
use crate::commands::fs::transfer::Prog;
use crate::errors::{code, AppError, AppResult};
use crate::vfs::{
    self,
    caps::{self, Capability},
};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use zip::{write::SimpleFileOptions, ZipWriter};

/// How many skipped entries are named individually. The count is complete;
/// the list is not, so a directory of a million refused names cannot grow it.
const MAX_REPORTED_SKIPS: usize = 100;

/// What could not be packed, and how much of it there was.
#[derive(Default)]
struct Skips {
    reported: Vec<ItemError>,
    total: u32,
}

impl Skips {
    fn note(&mut self, path: &Path, error: AppError) {
        self.total = self.total.saturating_add(1);
        if self.reported.len() < MAX_REPORTED_SKIPS {
            self.reported
                .push(ItemError::new(path.to_string_lossy().as_ref(), error));
        }
    }

    /// The reason to give when nothing at all could be packed.
    fn first_error(&self) -> AppError {
        match self.reported.first() {
            Some(first) => first.error.clone(),
            None => AppError::new(code::VALIDATION_BAD_INPUT)
                .detail("no source could be added to the archive"),
        }
    }
}

/// Whether an error ends the whole archive instead of one entry.
///
/// A limit is the bound the process promised to stay inside, and a cancel is
/// the user asking to stop: skipping past either would defeat it.
fn fatal(error: &AppError) -> bool {
    error.is(code::ARCHIVE_LIMIT) || error.is(code::CANCELLED)
}

/// A source that changed under the packer between the two passes.
fn changed() -> AppError {
    AppError::new(code::FS_IO).detail("source changed during archive creation")
}

fn cancelled(prog: &Prog) -> AppResult<()> {
    if prog.cancelled() {
        Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "archive creation cancelled",
        )
        .into())
    } else {
        Ok(())
    }
}

pub(crate) fn run(
    sources: &[String],
    destination: &str,
    prog: &mut Prog,
    limits: safety::Limits,
    check: &dyn Fn(&Path) -> AppResult<()>,
) -> AppResult<(u32, Vec<ItemError>)> {
    run_with_recovery(sources, destination, prog, limits, check, None)
}

/// One entry's outcome: packed, or left out with the reason to report.
enum Outcome {
    Packed,
    Skipped(AppError),
}

pub(crate) fn run_with_recovery(
    sources: &[String],
    destination: &str,
    prog: &mut Prog,
    limits: safety::Limits,
    check: &dyn Fn(&Path) -> AppResult<()>,
    recovery: Option<&crate::commands::fs::transfer::recovery::Handle>,
) -> AppResult<(u32, Vec<ItemError>)> {
    let _cache_mutation = crate::commands::fs::cache::Mutation::begin();
    if sources.is_empty() {
        return Err(AppError::new(code::VALIDATION_BAD_INPUT)
            .detail("at least one archive source is required"));
    }
    // Bound caller-supplied roots before any capability or filesystem work.
    if sources.len() as u64 > limits.entries {
        return Err(safety::limit());
    }
    caps::require(destination, Capability::ArchivePackTo)?;
    let (_, destination) = vfs::resolve(destination)?;
    let destination = destination.native()?;
    let parent = fs::canonicalize(destination.parent().unwrap_or(Path::new(".")))?;
    let name = destination
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(safety::unsafe_entry)?;
    let destination = parent.join(name);
    check(&destination)?;
    let mut skips = Skips::default();
    let mut pending = Vec::new();
    for raw in sources {
        caps::require(raw, Capability::ArchivePackFrom)?;
        let (_, source) = vfs::resolve(raw)?;
        let source = source.native()?.to_path_buf();
        check(&source)?;
        let source = match fs::symlink_metadata(&source) {
            Ok(meta) if meta.file_type().is_symlink() => {
                skips.note(&source, safety::unsafe_entry());
                continue;
            }
            Ok(_) => match fs::canonicalize(&source) {
                Ok(source) => source,
                Err(error) => {
                    skips.note(&source, error.into());
                    continue;
                }
            },
            Err(error) => {
                skips.note(&source, error.into());
                continue;
            }
        };
        // Not a skip: an archive written inside its own source cannot be
        // produced at all, whichever entries are left out of it.
        if destination.starts_with(&source) {
            return Err(AppError::new(code::VALIDATION_BAD_INPUT)
                .detail("archive destination overlaps a source"));
        }
        match source.file_name().and_then(|n| n.to_str()) {
            Some(name) => pending.push((source.clone(), name.to_owned())),
            None => skips.note(&source, safety::unsafe_entry()),
        }
    }
    let mut manifest = safety::Manifest::new(limits.clone(), limits.total_bytes);
    let mut entries = Vec::new();
    while let Some((path, name)) = pending.pop() {
        cancelled(prog)?;
        check(&path)?;
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) => {
                skips.note(&path, error.into());
                continue;
            }
        };
        let directory = meta.is_dir();
        // A link, a socket, a FIFO, a device: nothing a ZIP member can hold.
        if meta.file_type().is_symlink() || (!directory && !meta.is_file()) {
            skips.note(&path, safety::unsafe_entry());
            continue;
        }
        let size = if directory { 0 } else { meta.len() };
        match manifest.add(&name, directory, size, false) {
            Ok(_) => {}
            Err(error) if fatal(&error) => return Err(error),
            Err(error) => {
                skips.note(&path, error);
                continue;
            }
        }
        if !directory {
            prog.files_total = prog.files_total.checked_add(1).ok_or_else(safety::limit)?;
            prog.bytes_total = prog
                .bytes_total
                .checked_add(size)
                .ok_or_else(safety::limit)?;
        }
        entries.push((path.clone(), name.clone(), directory, size));
        if !directory {
            continue;
        }
        // An unreadable directory still becomes an entry — the archive then
        // records that the folder was there and empty, rather than nothing.
        let children = match fs::read_dir(&path) {
            Ok(children) => children,
            Err(error) => {
                skips.note(&path, error.into());
                continue;
            }
        };
        for child in children {
            let child = match child {
                Ok(child) => child,
                Err(error) => {
                    skips.note(&path, error.into());
                    continue;
                }
            };
            let Ok(child_name) = child.file_name().into_string() else {
                skips.note(&child.path(), safety::unsafe_entry());
                continue;
            };
            pending.push((child.path(), format!("{name}/{child_name}")));
            if pending.len().saturating_add(entries.len()) as u64 > limits.entries {
                return Err(safety::limit());
            }
        }
    }
    // Nothing packable is a failure, not an empty archive: the destination is
    // left alone and the user is told why, with the first reason as the error.
    if entries.is_empty() {
        return Err(skips.first_error());
    }
    crate::commands::fs::transfer::preflight::archive_capacity(
        &parent.clone().into(),
        crate::commands::fs::transfer::recovery::types::RecoveryKind::Pack,
        prog.bytes_total,
        entries.len().try_into().map_err(|_| safety::limit())?,
    )?;
    let mut stage = staging::Stage::new_with_recovery(&parent, recovery)?;
    let result = (|| -> AppResult<u32> {
        let output = stage.file(name, &limits)?.into_std();
        let mut zip = ZipWriter::new(output);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (path, name, directory, size) in entries {
            cancelled(prog)?;
            check(&path)?;
            match pack_one(
                &mut zip, &path, &name, directory, size, options, prog, check,
            )? {
                Outcome::Packed => {}
                Outcome::Skipped(error) => skips.note(&path, error),
            }
        }
        zip.finish()?.sync_all()?;
        crate::commands::fs::transfer::recovery::fault("flush");
        cancelled(prog)?;
        check(&destination)?;
        stage.commit(&prog.cancel)?;
        Ok(prog.files_done)
    })();
    match (result, stage.cleanup()) {
        (Ok(done), Ok(())) => Ok((done, skips.reported)),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(error), Err(cleanup)) => {
            Err(AppError::new(code::FS_IO).detail(format!("{error}; {cleanup}")))
        }
    }
}

/// Writes one collected entry, re-checking it against the collection pass.
///
/// `Err` ends the archive — a limit, a cancel, an authorization that changed
/// or a ZIP writer that failed. `Outcome::Skipped` is this entry alone, and a
/// member already started is removed again so no truncated file is left under
/// a name that promises a whole one.
#[allow(clippy::too_many_arguments)]
fn pack_one(
    zip: &mut ZipWriter<File>,
    path: &Path,
    name: &str,
    directory: bool,
    size: u64,
    options: SimpleFileOptions,
    prog: &mut Prog,
    check: &dyn Fn(&Path) -> AppResult<()>,
) -> AppResult<Outcome> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) => return Ok(Outcome::Skipped(error.into())),
    };
    if meta.file_type().is_symlink() {
        return Ok(Outcome::Skipped(safety::unsafe_entry()));
    }
    if meta.is_dir() != directory || (!directory && meta.len() != size) {
        return Ok(Outcome::Skipped(changed()));
    }
    if directory {
        zip.add_directory(format!("{name}/"), options)?;
        return Ok(Outcome::Packed);
    }
    let mut input = match File::open(path) {
        Ok(input) => input,
        Err(error) => return Ok(Outcome::Skipped(error.into())),
    };
    zip.start_file(name, options)?;
    let mut done = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    prog.emit(name, 0, size, true);
    loop {
        cancelled(prog)?;
        prog.wait_if_paused(name, done, size);
        cancelled(prog)?;
        check(path)?;
        let bytes = match input.read(&mut buffer) {
            Ok(bytes) => bytes,
            Err(error) => {
                zip.abort_file()?;
                return Ok(Outcome::Skipped(error.into()));
            }
        };
        if bytes == 0 {
            break;
        }
        done = done.checked_add(bytes as u64).ok_or_else(safety::limit)?;
        if done > size {
            zip.abort_file()?;
            return Ok(Outcome::Skipped(changed()));
        }
        zip.write_all(&buffer[..bytes])?;
        crate::commands::fs::transfer::recovery::fault("write");
        prog.bytes_done += bytes as u64;
        prog.emit(name, done, size, false);
    }
    if done != size {
        zip.abort_file()?;
        return Ok(Outcome::Skipped(changed()));
    }
    prog.files_done += 1;
    prog.emit(name, done, size, true);
    Ok(Outcome::Packed)
}
