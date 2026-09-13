//! Shared ZIP creation: validated inputs, bounded collection, private staging,
//! and publication only after the archive is finished and synced (TICKET-070).
use super::{safety, staging};
use crate::commands::fs::transfer::Prog;
use crate::errors::{code, AppError, AppResult};
use crate::vfs::{
    self,
    caps::{self, Capability},
};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::{write::SimpleFileOptions, ZipWriter};

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
) -> AppResult<u32> {
    run_with_recovery(sources, destination, prog, limits, check, None)
}

pub(crate) fn run_with_recovery(
    sources: &[String],
    destination: &str,
    prog: &mut Prog,
    limits: safety::Limits,
    check: &dyn Fn(&Path) -> AppResult<()>,
    recovery: Option<&crate::commands::fs::transfer::recovery::Handle>,
) -> AppResult<u32> {
    if sources.is_empty() {
        return Err(AppError::new(code::VALIDATION_BAD_INPUT)
            .detail("at least one archive source is required"));
    }
    caps::require(destination, Capability::ArchivePackTo)?;
    let (_, destination) = vfs::resolve(destination)?;
    let destination = Path::new(&destination);
    let parent = fs::canonicalize(destination.parent().unwrap_or(Path::new(".")))?;
    let name = destination
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(safety::unsafe_entry)?;
    let destination = parent.join(name);
    check(&destination)?;
    let mut pending = Vec::new();
    for raw in sources {
        caps::require(raw, Capability::ArchivePackFrom)?;
        let (_, source) = vfs::resolve(raw)?;
        let source = PathBuf::from(source);
        check(&source)?;
        let source_meta = fs::symlink_metadata(&source)?;
        if source_meta.file_type().is_symlink() {
            return Err(safety::unsafe_entry());
        }
        let source = fs::canonicalize(source)?;
        if destination.starts_with(&source) {
            return Err(AppError::new(code::VALIDATION_BAD_INPUT)
                .detail("archive destination overlaps a source"));
        }
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(safety::unsafe_entry)?
            .to_owned();
        pending.push((source, name));
    }
    let mut manifest = safety::Manifest::new(limits.clone(), limits.total_bytes);
    let mut entries = Vec::new();
    while let Some((path, name)) = pending.pop() {
        cancelled(prog)?;
        check(&path)?;
        let meta = fs::symlink_metadata(&path)?;
        let directory = meta.is_dir();
        manifest.add(
            &name,
            directory,
            if directory { 0 } else { meta.len() },
            !directory && !meta.is_file(),
        )?;
        if directory {
            for child in fs::read_dir(&path)? {
                let child = child?;
                let child_name = child
                    .file_name()
                    .into_string()
                    .map_err(|_| safety::unsafe_entry())?;
                pending.push((child.path(), format!("{name}/{child_name}")));
                if pending.len().saturating_add(entries.len()) as u64 > limits.entries {
                    return Err(safety::limit());
                }
            }
        } else {
            prog.files_total = prog.files_total.checked_add(1).ok_or_else(safety::limit)?;
            prog.bytes_total = prog
                .bytes_total
                .checked_add(meta.len())
                .ok_or_else(safety::limit)?;
        }
        entries.push((path, name, directory, meta.len()));
    }
    let mut stage = staging::Stage::new_with_recovery(&parent, recovery)?;
    let result = (|| -> AppResult<u32> {
        let output = stage.file(name, &limits)?.into_std();
        let mut zip = ZipWriter::new(output);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (path, name, directory, size) in entries {
            cancelled(prog)?;
            check(&path)?;
            let meta = fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink()
                || meta.is_dir() != directory
                || (!directory && meta.len() != size)
            {
                return Err(safety::unsafe_entry());
            }
            if directory {
                zip.add_directory(format!("{name}/"), options)?;
                continue;
            }
            zip.start_file(&name, options)?;
            let mut input = File::open(&path)?;
            let mut done = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            prog.emit(&name, 0, size, true);
            loop {
                cancelled(prog)?;
                prog.wait_if_paused(&name, done, size);
                cancelled(prog)?;
                check(&path)?;
                let bytes = input.read(&mut buffer)?;
                if bytes == 0 {
                    break;
                }
                done = done.checked_add(bytes as u64).ok_or_else(safety::limit)?;
                if done > size {
                    return Err(safety::limit());
                }
                zip.write_all(&buffer[..bytes])?;
                crate::commands::fs::transfer::recovery::fault("write");
                prog.bytes_done += bytes as u64;
                prog.emit(&name, done, size, false);
            }
            if done != size {
                return Err(
                    AppError::new(code::FS_IO).detail("source changed during archive creation")
                );
            }
            prog.files_done += 1;
            prog.emit(&name, done, size, true);
        }
        zip.finish()?.sync_all()?;
        crate::commands::fs::transfer::recovery::fault("flush");
        cancelled(prog)?;
        check(&destination)?;
        stage.commit(&prog.cancel)?;
        Ok(prog.files_done)
    })();
    match (result, stage.cleanup()) {
        (result, Ok(())) => result,
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(error), Err(cleanup)) => {
            Err(AppError::new(code::FS_IO).detail(format!("{error}; {cleanup}")))
        }
    }
}
