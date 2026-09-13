//! Archive-format adapters for the shared validation and staging layers.
use super::super::transfer::{events::ItemError, Prog};
use super::{
    archive_kind, inner_prefix, is_password_error, map_7z_err, open_tar_reader, password,
    password_required, password_wrong, safety, staging, tar_raw, ArchiveKind,
};
use crate::errors::{code, AppError, AppResult};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use zip::{result::ZipError, ZipArchive};

/// Checks whether an entry is to be extracted and returns its target path
/// relative to the chosen level (`base`).
pub(super) fn match_target(
    name: &str,
    base: &str,
    all: bool,
    targets: &HashSet<String>,
) -> Option<String> {
    if !name.starts_with(base) {
        return None;
    }
    let rel = &name[base.len()..];
    if rel.is_empty() {
        return None;
    }
    let first = rel.split('/').next().unwrap_or("");
    if all || targets.contains(first) {
        Some(rel.to_string())
    } else {
        None
    }
}

pub(super) fn preflight(
    archive: &str,
    pw: &Option<String>,
    limits: safety::Limits,
    cancel: &std::sync::atomic::AtomicBool,
) -> AppResult<safety::Manifest> {
    check_cancel(cancel)?;
    let mut manifest = safety::Manifest::new(limits, fs::metadata(archive)?.len());
    match archive_kind(archive) {
        Some(ArchiveKind::Zip) => {
            let mut zip = ZipArchive::new(File::open(archive)?)?;
            for index in 0..zip.len() {
                check_cancel(cancel)?;
                let entry = zip.by_index_raw(index)?;
                let link = entry.unix_mode().is_some_and(|mode| {
                    let kind = mode & 0o170000;
                    kind != 0 && kind != 0o100000 && kind != 0o040000
                });
                manifest.add(entry.name(), entry.is_dir(), entry.size(), link)?;
            }
        }
        Some(kind @ (ArchiveKind::Tar | ArchiveKind::TarGz | ArchiveKind::TarXz)) => {
            let mut tar = tar::Archive::new(open_tar_reader(kind, archive)?);
            for entry in tar.entries()? {
                check_cancel(cancel)?;
                let entry = entry?;
                let kind = entry.header().entry_type();
                let bytes = entry.path_bytes();
                let name = std::str::from_utf8(&bytes).map_err(|_| safety::unsafe_entry())?;
                manifest.add(
                    name,
                    kind.is_dir(),
                    entry.header().size()?,
                    !kind.is_file() && !kind.is_dir(),
                )?;
            }
        }
        Some(ArchiveKind::SevenZ) => {
            let meta = sevenz_rust::Archive::open_with_password(archive, &password(pw))
                .map_err(map_7z_err)?;
            for entry in &meta.files {
                check_cancel(cancel)?;
                let mode = (entry.windows_attributes >> 16) & 0o170000;
                let link = entry.is_anti_item
                    || (entry.has_windows_attributes
                        && (entry.windows_attributes & 0x400 != 0
                            || (mode != 0 && mode != 0o100000 && mode != 0o040000)));
                manifest.add(&entry.name, entry.is_directory, entry.size, link)?;
            }
        }
        None => return Err(AppError::new(code::ARCHIVE_CORRUPT)),
    }
    Ok(manifest)
}

/// Writes a data stream as a file to `out` and reports progress.
pub(super) fn write_entry(
    reader: &mut dyn Read,
    stage: &mut staging::Stage,
    name: &str,
    file_total: u64,
    prog: &mut Prog,
    manifest: &mut safety::Manifest,
) -> AppResult<()> {
    let mut f = stage.file(name, &manifest.limits)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut file_done = 0u64;
    prog.emit(name, 0, file_total, true);
    loop {
        if prog.cancelled() {
            return Err(AppError::new(code::CANCELLED));
        }
        let n = reader.read(&mut buf).map_err(AppError::from)?;
        if n == 0 {
            break;
        }
        file_done = manifest.charge(file_done, n as u64, file_total)?;
        f.write_all(&buf[..n]).map_err(AppError::from)?;
        prog.bytes_done += n as u64;
        prog.emit(name, file_done, file_total, false);
    }
    if file_done != file_total {
        return Err(AppError::new(code::ARCHIVE_CORRUPT));
    }
    f.sync_all()?;
    prog.files_done += 1;
    prog.emit(name, file_total, file_total, true);
    Ok(())
}

/// Extracts matching entries from a ZIP archive.
pub(super) fn extract_zip(
    archive: &str,
    base: &str,
    limits: safety::Limits,
    targets: &HashSet<String>,
    dest: &mut staging::Stage,
    pw: &Option<String>,
    prog: &mut Prog,
) -> AppResult<(u32, Vec<ItemError>)> {
    let all = targets.is_empty();
    let mut manifest = preflight(archive, pw, limits, &prog.cancel)?;
    let open = || -> AppResult<ZipArchive<File>> {
        ZipArchive::new(File::open(archive).map_err(AppError::from)?).map_err(AppError::from)
    };

    // Pass 1: determine matching indices + totals, check encryption.
    let mut zip = open()?;
    let mut indices = Vec::new();
    let mut needs_pw = false;
    for i in 0..zip.len() {
        let e = zip.by_index_raw(i).map_err(AppError::from)?;
        if e.is_dir() {
            if let Some(rel) = match_target(e.name(), base, all, targets) {
                dest.directory(&rel, &manifest.limits)?;
            }
            continue;
        }
        let name = e.name().to_string();
        if match_target(&name, base, all, targets).is_some() {
            indices.push(i);
            prog.files_total += 1;
            prog.bytes_total += e.size();
            if e.encrypted() {
                needs_pw = true;
            }
        }
    }
    if needs_pw && pw.is_none() {
        return Err(password_required());
    }
    prog.emit("", 0, 0, true);

    // Pass 2: extract.
    let mut zip = open()?;
    let mut ok = 0u32;
    let mut errors = Vec::new();
    for i in indices {
        if prog.cancelled() {
            break;
        }
        let name = zip.name_for_index(i).unwrap_or_default().to_string();
        match extract_zip_one(&mut zip, i, base, dest, pw, prog, &mut manifest) {
            Ok(()) => ok += 1,
            // A wrong password is fatal → abort immediately and report.
            Err(e) if is_password_error(&e) => return Err(e),
            Err(e) => errors.push(ItemError::new(name, e)),
        }
    }
    Ok((ok, errors))
}

fn extract_zip_one(
    zip: &mut ZipArchive<File>,
    index: usize,
    base: &str,
    dest: &mut staging::Stage,
    pw: &Option<String>,
    prog: &mut Prog,
    manifest: &mut safety::Manifest,
) -> AppResult<()> {
    // Read metadata in its own scope (separate borrow).
    let (name, encrypted, size) = {
        let e = zip.by_index_raw(index).map_err(AppError::from)?;
        let mode = e.unix_mode().unwrap_or(0) & 0o170000;
        manifest.verify(
            e.name(),
            e.is_dir(),
            e.size(),
            mode != 0 && mode != 0o100000 && mode != 0o040000,
        )?;
        (e.name().to_string(), e.encrypted(), e.size())
    };
    let rel = &name[base.len()..];
    if encrypted {
        let pw = pw.as_deref().ok_or_else(password_required)?;
        let mut e = zip
            .by_index_decrypt(index, pw.as_bytes())
            .map_err(|err| match err {
                ZipError::InvalidPassword => password_wrong(),
                other => AppError::from(other),
            })?;
        write_entry(&mut e, dest, rel, size, prog, manifest)
    } else {
        let mut e = zip.by_index(index).map_err(AppError::from)?;
        write_entry(&mut e, dest, rel, size, prog, manifest)
    }
}

/// Extracts matching entries from a tar archive (decompressing if needed).
pub(super) fn extract_tar(
    kind: ArchiveKind,
    archive: &str,
    base: &str,
    limits: safety::Limits,
    targets: &HashSet<String>,
    dest: &mut staging::Stage,
    prog: &mut Prog,
) -> AppResult<(u32, Vec<ItemError>)> {
    let all = targets.is_empty();
    let mut manifest = preflight(archive, &None, limits, &prog.cancel)?;
    // Pass 1: totals from the headers.
    for (name, size, is_dir) in tar_raw(open_tar_reader(kind, archive)?)? {
        if is_dir {
            if let Some(rel) = match_target(&name, base, all, targets) {
                dest.directory(&rel, &manifest.limits)?;
            }
            continue;
        }
        if match_target(&name, base, all, targets).is_some() {
            prog.files_total += 1;
            prog.bytes_total += size;
        }
    }
    prog.emit("", 0, 0, true);

    // Pass 2: extract (reopen the reader, tar streams are not seekable).
    let mut ar = tar::Archive::new(open_tar_reader(kind, archive)?);
    let mut ok = 0u32;
    let mut errors = Vec::new();
    for entry in ar.entries().map_err(AppError::from)? {
        if prog.cancelled() {
            break;
        }
        let mut entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(ItemError::new("", e.into()));
                continue;
            }
        };
        let bytes = entry.path_bytes();
        let name = std::str::from_utf8(&bytes)
            .map_err(|_| safety::unsafe_entry())?
            .to_owned();
        let kind = entry.header().entry_type();
        let size = entry.header().size()?;
        manifest.verify(
            &name,
            kind.is_dir(),
            size,
            !kind.is_file() && !kind.is_dir(),
        )?;
        if kind.is_dir() {
            continue;
        }
        let rel = match match_target(&name, base, all, targets) {
            Some(r) => r,
            None => continue,
        };
        match write_entry(&mut entry, dest, &rel, size, prog, &mut manifest) {
            Ok(()) => ok += 1,
            Err(e) => errors.push(ItemError::new(&name, e)),
        }
    }
    Ok((ok, errors))
}

/// Extracts matching entries from a 7z archive (with a password if needed).
pub(super) fn extract_7z(
    archive: &str,
    base: &str,
    limits: safety::Limits,
    targets: &HashSet<String>,
    dest: &mut staging::Stage,
    pw: &Option<String>,
    prog: &mut Prog,
) -> AppResult<(u32, Vec<ItemError>)> {
    let all = targets.is_empty();
    let mut manifest = preflight(archive, pw, limits, &prog.cancel)?;
    // Pass 1: totals from the metadata (also opens with the password).
    let meta =
        sevenz_rust::Archive::open_with_password(archive, &password(pw)).map_err(map_7z_err)?;
    for f in &meta.files {
        if f.is_directory {
            if let Some(rel) = match_target(&f.name, base, all, targets) {
                dest.directory(&rel, &manifest.limits)?;
            }
            continue;
        }
        let name = f.name.replace('\\', "/");
        if match_target(&name, base, all, targets).is_some() {
            prog.files_total += 1;
            prog.bytes_total += f.size;
        }
    }
    prog.emit("", 0, 0, true);

    // Pass 2: extract via the streaming reader.
    let mut reader = sevenz_rust::SevenZReader::open(archive, password(pw)).map_err(map_7z_err)?;
    let mut ok = 0u32;
    let mut errors = Vec::new();
    reader
        .for_each_entries(|entry, rd| {
            if prog.cancelled() {
                return Ok(false);
            }
            let mode = (entry.windows_attributes >> 16) & 0o170000;
            let link = entry.is_anti_item
                || (entry.has_windows_attributes
                    && (entry.windows_attributes & 0x400 != 0
                        || (mode != 0 && mode != 0o100000 && mode != 0o040000)));
            if let Err(error) = manifest.verify(&entry.name, entry.is_directory, entry.size, link) {
                errors.push(ItemError::new(&entry.name, error));
                return Ok(false);
            }
            if entry.is_directory {
                return Ok(true);
            }
            let name = entry.name.clone();
            let rel = match match_target(&name, base, all, targets) {
                Some(r) => r,
                None => return Ok(true),
            };
            match write_entry(rd, dest, &rel, entry.size, prog, &mut manifest) {
                Ok(()) => ok += 1,
                Err(e) => errors.push(ItemError::new(&name, e)),
            }
            Ok(true)
        })
        .map_err(map_7z_err)?;
    Ok((ok, errors))
}

fn check_cancel(cancel: &std::sync::atomic::AtomicBool) -> AppResult<()> {
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        Err(AppError::new(code::CANCELLED))
    } else {
        Ok(())
    }
}

pub(crate) struct ExtractJob {
    pub archive: String,
    pub base_inner: String,
    pub names: Vec<String>,
    pub dest_dir: String,
    pub pw: Option<String>,
}

/// Shared extraction service, independent of the Tauri event bus and window.
pub(crate) fn run(
    job: ExtractJob,
    prog: &mut Prog,
    limits: safety::Limits,
) -> AppResult<(u32, Vec<ItemError>)> {
    use crate::vfs::caps::{self, Capability};
    let ExtractJob {
        archive,
        base_inner,
        names,
        dest_dir,
        pw,
    } = job;
    caps::require_pair(
        &archive,
        Capability::ArchiveExtractFrom,
        &dest_dir,
        Capability::ArchiveExtractTo,
    )?;
    let base = inner_prefix(&base_inner);
    let targets: HashSet<String> = names.into_iter().collect();
    check_cancel(&prog.cancel)?;
    let mut stage = staging::Stage::new(std::path::Path::new(&dest_dir))?;
    let result = match archive_kind(&archive) {
        Some(ArchiveKind::Zip) => {
            extract_zip(&archive, &base, limits, &targets, &mut stage, &pw, prog)
        }
        Some(kind @ (ArchiveKind::Tar | ArchiveKind::TarGz | ArchiveKind::TarXz)) => {
            extract_tar(kind, &archive, &base, limits, &targets, &mut stage, prog)
        }
        Some(ArchiveKind::SevenZ) => {
            extract_7z(&archive, &base, limits, &targets, &mut stage, &pw, prog)
        }
        None => Err(AppError::new(code::ARCHIVE_UNKNOWN_FORMAT)),
    };
    let (mut ok, mut errors) = match result {
        Ok(result) => result,
        Err(error) => (0, vec![ItemError::new(&archive, error)]),
    };
    if !prog.cancelled() && errors.is_empty() && prog.files_done != prog.files_total {
        errors.push(ItemError::new(
            &archive,
            AppError::new(code::ARCHIVE_CORRUPT).op("extract-incomplete"),
        ));
    }
    if !prog.cancelled() && errors.is_empty() {
        if let Err(error) = stage.commit(&prog.cancel) {
            errors.push(ItemError::new(&dest_dir, error));
        }
    }
    if prog.cancelled() || !errors.is_empty() {
        ok = 0;
    }
    if let Err(error) = stage.cleanup() {
        errors.push(ItemError::new(&dest_dir, error));
    }
    Ok((ok, errors))
}
