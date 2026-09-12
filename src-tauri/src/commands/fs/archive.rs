// ============================================================
// Archive handling: browse, extract, pack.
// Supported for reading/extracting: ZIP (incl. AES/ZipCrypto
// password), tar, tar.gz, tar.xz, and 7z (incl. password).
// Packing still produces ZIP archives.
// Uses the transfer infrastructure (Prog, spawn_op, OpDone) from
// the file module and the listing types from the dir module.
// ============================================================

use crate::errors::{code, AppError, AppResult, ErrorAction};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use super::attrs;
use super::dir::{DirEntry, DirListing};
use super::transfer::events::ItemError;
use super::transfer::{spawn_op, OpDone, Prog};
use crate::vfs::caps::{self, Capability};

/// The archive is encrypted and no password was given; the frontend
/// asks for one. Since ticket 036 this is a real error code — the
/// sentinel string it replaced was matched by substring, which made a
/// password prompt depend on message wording.
fn password_required() -> AppError {
    AppError::new(code::ARCHIVE_PASSWORD_REQUIRED).action(ErrorAction::ProvidePassword)
}

/// The password that was given is wrong.
fn password_wrong() -> AppError {
    AppError::new(code::ARCHIVE_PASSWORD_WRONG).action(ErrorAction::ProvidePassword)
}

/// Whether an error is one of the two password outcomes. The one place
/// that decides it, for both the 7z and the ZIP path.
fn is_password_error(e: &AppError) -> bool {
    e.is(code::ARCHIVE_PASSWORD_REQUIRED) || e.is(code::ARCHIVE_PASSWORD_WRONG)
}

/// Supported archive formats (detected by file extension).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    Tar,
    TarGz,
    TarXz,
    SevenZ,
}

/// Detects the archive format from the (double) extension.
fn archive_kind(path: &str) -> Option<ArchiveKind> {
    let l = path.to_lowercase();
    if l.ends_with(".zip") {
        Some(ArchiveKind::Zip)
    } else if l.ends_with(".tar.gz") || l.ends_with(".tgz") {
        Some(ArchiveKind::TarGz)
    } else if l.ends_with(".tar.xz") || l.ends_with(".txz") {
        Some(ArchiveKind::TarXz)
    } else if l.ends_with(".tar") {
        Some(ArchiveKind::Tar)
    } else if l.ends_with(".7z") {
        Some(ArchiveKind::SevenZ)
    } else {
        None
    }
}

/// Raw archive member: entry name (with "/" separators), size, folder flag.
type RawEntry = (String, u64, bool);

/// Normalizes an inner path to a prefix ("" or "sub/").
fn inner_prefix(inner: &str) -> String {
    let trimmed = inner.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{}/", trimmed)
    }
}

/// Builds a `sevenz_rust::Password` from an optional string.
fn password(opt: &Option<String>) -> sevenz_rust::Password {
    match opt {
        Some(s) => sevenz_rust::Password::from(s.as_str()),
        None => sevenz_rust::Password::empty(),
    }
}

/// The 7z boundary: its two password outcomes become codes, everything
/// else a corrupt-archive error keeping the library's own wording as
/// redacted detail.
fn map_7z_err(e: sevenz_rust::Error) -> AppError {
    match e {
        sevenz_rust::Error::PasswordRequired => password_required(),
        sevenz_rust::Error::MaybeBadPassword(_) => password_wrong(),
        other => AppError::new(code::ARCHIVE_CORRUPT)
            .action(ErrorAction::Report)
            .detail(other),
    }
}

/// Opens a streaming reader for a tar archive (decompressing if needed).
fn open_tar_reader(kind: ArchiveKind, archive: &str) -> AppResult<Box<dyn Read>> {
    let f = File::open(archive).map_err(AppError::from)?;
    Ok(match kind {
        ArchiveKind::Tar => Box::new(f),
        ArchiveKind::TarGz => Box::new(flate2::read::GzDecoder::new(f)),
        ArchiveKind::TarXz => Box::new(xz2::read::XzDecoder::new(f)),
        _ => unreachable!("open_tar_reader nur für tar-Varianten"),
    })
}

// ---------- Listing ----------

/// Reads the member list of a ZIP archive (metadata only, no password).
fn zip_raw(archive: &str) -> AppResult<Vec<RawEntry>> {
    let file = File::open(archive).map_err(AppError::from)?;
    let mut zip = ZipArchive::new(file).map_err(AppError::from)?;
    let mut out = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        // by_index_raw reads metadata even for encrypted entries.
        let e = zip.by_index_raw(i).map_err(AppError::from)?;
        out.push((e.name().to_string(), e.size(), e.is_dir()));
    }
    Ok(out)
}

/// Reads the member list of a tar archive (headers, without data streams).
fn tar_raw(reader: Box<dyn Read>) -> AppResult<Vec<RawEntry>> {
    let mut ar = tar::Archive::new(reader);
    let mut out = Vec::new();
    for entry in ar.entries().map_err(AppError::from)? {
        let entry = entry.map_err(AppError::from)?;
        let is_dir = entry.header().entry_type().is_dir();
        let name = entry
            .path()
            .map_err(AppError::from)?
            .to_string_lossy()
            .replace('\\', "/");
        let size = entry.header().size().unwrap_or(0);
        out.push((name, size, is_dir));
    }
    Ok(out)
}

/// Reads the member list of a 7z archive (with a password if needed).
fn sevenz_raw(archive: &str, pw: &Option<String>) -> AppResult<Vec<RawEntry>> {
    let ar =
        sevenz_rust::Archive::open_with_password(archive, &password(pw)).map_err(map_7z_err)?;
    Ok(ar
        .files
        .iter()
        .map(|f| (f.name.replace('\\', "/"), f.size, f.is_directory))
        .collect())
}

/// Builds the view of the level `inner` from the raw member list.
fn collapse(raw: &[RawEntry], inner: &str) -> DirListing {
    let prefix = inner_prefix(inner);
    let mut dirs = BTreeSet::new();
    let mut files: Vec<DirEntry> = Vec::new();
    let mut seen = HashSet::new();

    for (name, size, is_dir) in raw {
        if !name.starts_with(&prefix) {
            continue;
        }
        let rest = name[prefix.len()..].trim_end_matches('/');
        if rest.is_empty() {
            continue;
        }
        match rest.find('/') {
            // Sub-entry → the top level is a folder.
            Some(idx) => {
                dirs.insert(rest[..idx].to_string());
            }
            None if *is_dir => {
                dirs.insert(rest.to_string());
            }
            None => {
                if seen.insert(rest.to_string()) {
                    files.push(DirEntry {
                        hidden: attrs::is_hidden_name(rest),
                        name: rest.to_string(),
                        is_dir: false,
                        is_symlink: false,
                        size: *size,
                        modified: 0,
                        mode: None,
                        readonly: false,
                        executable: false,
                    });
                }
            }
        }
    }
    for d in dirs {
        files.push(DirEntry {
            hidden: attrs::is_hidden_name(&d),
            name: d,
            is_dir: true,
            is_symlink: false,
            size: 0,
            modified: 0,
            mode: None,
            readonly: false,
            executable: false,
        });
    }

    DirListing {
        path: format!("/{}", inner.trim_matches('/')),
        entries: files,
    }
}

/// Lists the contents of an archive at the level `inner`.
#[tauri::command]
pub fn list_archive(
    archive: String,
    inner: String,
    password: Option<String>,
) -> AppResult<DirListing> {
    // Every reader below opens the archive with `std::fs` (TICKET-031);
    // browsing an archive that lives on a remote share needs the
    // archive commands routed through the VFS first.
    caps::require(&archive, Capability::ArchiveBrowse)?;
    let kind = archive_kind(&archive).ok_or_else(|| "Unbekanntes Archivformat".to_string())?;
    let raw = match kind {
        ArchiveKind::Zip => zip_raw(&archive)?,
        ArchiveKind::Tar | ArchiveKind::TarGz | ArchiveKind::TarXz => {
            tar_raw(open_tar_reader(kind, &archive)?)?
        }
        ArchiveKind::SevenZ => sevenz_raw(&archive, &password)?,
    };
    Ok(collapse(&raw, &inner))
}

// ---------- Extracting ----------

/// Checks whether an entry is to be extracted and returns its target path
/// relative to the chosen level (`base`).
fn match_target(name: &str, base: &str, all: bool, targets: &HashSet<String>) -> Option<String> {
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

/// Writes a data stream as a file to `out` and reports progress.
fn write_entry(
    reader: &mut dyn Read,
    out: &Path,
    name: &str,
    file_total: u64,
    prog: &mut Prog,
) -> AppResult<()> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(AppError::from)?;
    }
    let mut f = File::create(out).map_err(AppError::from)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut file_done = 0u64;
    prog.emit(name, 0, file_total, true);
    loop {
        if prog.cancelled() {
            break;
        }
        let n = reader.read(&mut buf).map_err(AppError::from)?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n]).map_err(AppError::from)?;
        file_done += n as u64;
        prog.bytes_done += n as u64;
        prog.emit(name, file_done, file_total, false);
    }
    prog.files_done += 1;
    prog.emit(name, file_total, file_total, true);
    Ok(())
}

/// Extracts matching entries from a ZIP archive.
fn extract_zip(
    archive: &str,
    base: &str,
    all: bool,
    targets: &HashSet<String>,
    dest: &Path,
    pw: &Option<String>,
    prog: &mut Prog,
) -> AppResult<(u32, Vec<ItemError>)> {
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
        match extract_zip_one(&mut zip, i, base, dest, pw, prog) {
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
    dest: &Path,
    pw: &Option<String>,
    prog: &mut Prog,
) -> AppResult<()> {
    // Read metadata in its own scope (separate borrow).
    let (name, encrypted, size) = {
        let e = zip.by_index_raw(index).map_err(AppError::from)?;
        (e.name().to_string(), e.encrypted(), e.size())
    };
    let rel = &name[base.len()..];
    let out = dest.join(rel);
    if encrypted {
        let pw = pw.as_deref().ok_or_else(password_required)?;
        let mut e = zip
            .by_index_decrypt(index, pw.as_bytes())
            .map_err(|err| match err {
                ZipError::InvalidPassword => password_wrong(),
                other => AppError::from(other),
            })?;
        write_entry(&mut e, &out, &name, size, prog)
    } else {
        let mut e = zip.by_index(index).map_err(AppError::from)?;
        write_entry(&mut e, &out, &name, size, prog)
    }
}

/// Extracts matching entries from a tar archive (decompressing if needed).
fn extract_tar(
    kind: ArchiveKind,
    archive: &str,
    base: &str,
    all: bool,
    targets: &HashSet<String>,
    dest: &Path,
    prog: &mut Prog,
) -> AppResult<(u32, Vec<ItemError>)> {
    // Pass 1: totals from the headers.
    for (name, size, is_dir) in tar_raw(open_tar_reader(kind, archive)?)? {
        if is_dir {
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
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let name = match entry.path() {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(e) => {
                errors.push(ItemError::new("", e.into()));
                continue;
            }
        };
        let rel = match match_target(&name, base, all, targets) {
            Some(r) => r,
            None => continue,
        };
        let size = entry.header().size().unwrap_or(0);
        let out = dest.join(&rel);
        match write_entry(&mut entry, &out, &name, size, prog) {
            Ok(()) => ok += 1,
            Err(e) => errors.push(ItemError::new(&name, e)),
        }
    }
    Ok((ok, errors))
}

/// Extracts matching entries from a 7z archive (with a password if needed).
fn extract_7z(
    archive: &str,
    base: &str,
    all: bool,
    targets: &HashSet<String>,
    dest: &Path,
    pw: &Option<String>,
    prog: &mut Prog,
) -> AppResult<(u32, Vec<ItemError>)> {
    // Pass 1: totals from the metadata (also opens with the password).
    let meta =
        sevenz_rust::Archive::open_with_password(archive, &password(pw)).map_err(map_7z_err)?;
    for f in &meta.files {
        if f.is_directory {
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
            if entry.is_directory {
                return Ok(true);
            }
            let name = entry.name.replace('\\', "/");
            let rel = match match_target(&name, base, all, targets) {
                Some(r) => r,
                None => return Ok(true),
            };
            let out = dest.join(&rel);
            match write_entry(rd, &out, &name, entry.size, prog) {
                Ok(()) => ok += 1,
                Err(e) => errors.push(ItemError::new(&name, e)),
            }
            Ok(true)
        })
        .map_err(map_7z_err)?;
    Ok((ok, errors))
}

/// What to extract and where to. Carried as one value because the five
/// parts are one request — the archive, the level inside it, the chosen
/// members, the destination and the password — and splitting them
/// across a parameter list only made the signature harder to read.
struct ExtractJob {
    archive: String,
    /// Level inside the archive the names are relative to.
    base_inner: String,
    /// Chosen members; empty means the whole level.
    names: Vec<String>,
    dest_dir: String,
    pw: Option<String>,
}

/// Extracts selected entries (or the whole archive if `names` is empty).
fn run_extract(
    app: AppHandle,
    id: String,
    job: ExtractJob,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
) {
    let ExtractJob {
        archive,
        base_inner,
        names,
        dest_dir,
        pw,
    } = job;
    // Source archive and destination folder are separate questions
    // (TICKET-031). A refusal is reported through the same `fs-done`
    // event as any other failure, so the transfer window shows it.
    if let Err(e) = caps::require_pair(
        &archive,
        Capability::ArchiveExtractFrom,
        &dest_dir,
        Capability::ArchiveExtractTo,
    ) {
        return fail_op(&app, id, "extract", e);
    }
    let dest = PathBuf::from(&dest_dir);
    let base = inner_prefix(&base_inner);
    let targets: HashSet<String> = names.into_iter().collect();
    let all = targets.is_empty();

    let mut prog = Prog {
        sink: Arc::new(app.clone()),
        id: id.clone(),
        op: "extract".into(),
        files_total: 0,
        bytes_total: 0,
        files_done: 0,
        bytes_done: 0,
        last: Instant::now(),
        cancel: cancel.clone(),
        pause: pause.clone(),
    };

    let result = match archive_kind(&archive) {
        Some(ArchiveKind::Zip) => {
            extract_zip(&archive, &base, all, &targets, &dest, &pw, &mut prog)
        }
        Some(kind @ (ArchiveKind::Tar | ArchiveKind::TarGz | ArchiveKind::TarXz)) => {
            extract_tar(kind, &archive, &base, all, &targets, &dest, &mut prog)
        }
        Some(ArchiveKind::SevenZ) => {
            extract_7z(&archive, &base, all, &targets, &dest, &pw, &mut prog)
        }
        None => Err(AppError::from("Unbekanntes Archivformat")),
    };

    let (ok, errors) = match result {
        Ok((ok, errors)) => (ok, errors),
        Err(e) => (0, vec![ItemError::new(&archive, e)]),
    };

    let cancelled = prog.cancelled();
    let _ = app.emit(
        "fs-done",
        OpDone {
            id,
            op: "extract".into(),
            ok,
            errors,
            cancelled,
            // Archives have no staging choice of their own.
            mode: String::new(),
        },
    );
}

// ---------- Packing (ZIP) ----------

/// Collects all files under `path` with their ZIP entry name.
fn collect_files(path: &Path, entry_name: &str, out: &mut Vec<(PathBuf, String)>) {
    if path.is_dir() {
        if let Ok(rd) = fs::read_dir(path) {
            for entry in rd.flatten() {
                let child = entry.file_name().to_string_lossy().into_owned();
                collect_files(&entry.path(), &format!("{}/{}", entry_name, child), out);
            }
        }
    } else {
        out.push((path.to_path_buf(), entry_name.to_string()));
    }
}

/// Packs multiple sources into a new ZIP archive.
fn run_pack(
    app: AppHandle,
    id: String,
    sources: Vec<String>,
    dest_zip: String,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
) {
    for src in &sources {
        if let Err(e) = caps::require_pair(
            src,
            Capability::ArchivePackFrom,
            &dest_zip,
            Capability::ArchivePackTo,
        ) {
            return fail_op(&app, id, "pack", e);
        }
    }
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    for s in &sources {
        let src = Path::new(s);
        let base = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "datei".into());
        collect_files(src, &base, &mut files);
    }
    let files_total = files.len() as u32;
    let bytes_total = files
        .iter()
        .map(|(p, _)| fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();

    let mut prog = Prog {
        sink: Arc::new(app.clone()),
        id: id.clone(),
        op: "pack".into(),
        files_total,
        bytes_total,
        files_done: 0,
        bytes_done: 0,
        last: Instant::now(),
        cancel: cancel.clone(),
        pause: pause.clone(),
    };
    prog.emit("", 0, 0, true);

    let mut ok = 0u32;
    let mut errors = Vec::new();
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    match File::create(&dest_zip) {
        Ok(out) => {
            let mut zip = ZipWriter::new(out);
            for (path, entry_name) in files {
                if prog.cancelled() {
                    break;
                }
                match pack_one(&mut zip, &path, &entry_name, opts, &mut prog) {
                    Ok(_) => ok += 1,
                    Err(e) => errors.push(ItemError::new(&entry_name, e)),
                }
            }
            if let Err(e) = zip.finish() {
                errors.push(ItemError::new("", e.into()));
            }
        }
        Err(e) => errors.push(ItemError::new(&dest_zip, e.into())),
    }

    // On abort, remove the incomplete archive.
    if prog.cancelled() {
        let _ = fs::remove_file(&dest_zip);
    }

    let cancelled = prog.cancelled();
    let _ = app.emit(
        "fs-done",
        OpDone {
            id,
            op: "pack".into(),
            ok,
            errors,
            cancelled,
            // Archives have no staging choice of their own.
            mode: String::new(),
        },
    );
}

fn pack_one(
    zip: &mut ZipWriter<File>,
    path: &Path,
    entry_name: &str,
    opts: SimpleFileOptions,
    prog: &mut Prog,
) -> AppResult<()> {
    zip.start_file(entry_name, opts).map_err(AppError::from)?;
    let file_total = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut f = File::open(path).map_err(AppError::from)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut file_done = 0u64;
    let name = path.to_string_lossy().into_owned();
    loop {
        let n = f.read(&mut buf).map_err(AppError::from)?;
        if n == 0 {
            break;
        }
        zip.write_all(&buf[..n]).map_err(AppError::from)?;
        file_done += n as u64;
        prog.bytes_done += n as u64;
        prog.emit(&name, file_done, file_total, false);
    }
    prog.files_done += 1;
    prog.emit(&name, file_total, file_total, true);
    Ok(())
}

/// Reports a refusal that happened before any work started, through the
/// same `fs-done` event the frontend already listens on — a blocked
/// operation must not leave the transfer window waiting forever.
fn fail_op(app: &AppHandle, id: String, op: &str, err: AppError) {
    let _ = app.emit(
        "fs-done",
        OpDone {
            id,
            op: op.to_string(),
            ok: 0,
            errors: vec![ItemError::new("", err)],
            cancelled: false,
            mode: String::new(),
        },
    );
}

// ---------- Commands ----------

/// Starts the extraction in the background.
#[tauri::command]
pub fn extract_entries(
    app: AppHandle,
    id: String,
    archive: String,
    base: String,
    names: Vec<String>,
    dest_dir: String,
    password: Option<String>,
) {
    let id2 = id.clone();
    let job = ExtractJob {
        archive,
        base_inner: base,
        names,
        dest_dir,
        pw: password,
    };
    spawn_op(id, move |cancel, pause| {
        run_extract(app, id2, job, cancel, pause)
    });
}

/// Starts the packing in the background.
#[tauri::command]
pub fn create_archive(app: AppHandle, id: String, sources: Vec<String>, dest_zip: String) {
    let id2 = id.clone();
    spawn_op(id, move |cancel, pause| {
        run_pack(app, id2, sources, dest_zip, cancel, pause)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// One format under test: its file name and the builder for it.
    type ArchiveCase = (&'static str, fn(&Path));

    // Expected contents of the test archives: one file in the root and
    // one in a subfolder.
    const HELLO: &[u8] = b"Hallo Welt";
    const DEEP: &[u8] = b"tief verschachtelt";

    /// A unique temporary working directory for a test.
    fn tmp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rc_arch_{}_{}", tag, nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes both test files as tar entries into a writer.
    fn write_tar_entries<W: Write>(builder: &mut tar::Builder<W>) {
        for (name, data) in [("hello.txt", HELLO), ("sub/deep.txt", DEEP)] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, data).unwrap();
        }
    }

    fn make_tar(path: &Path) {
        let mut b = tar::Builder::new(File::create(path).unwrap());
        write_tar_entries(&mut b);
        b.finish().unwrap();
    }

    fn make_tar_gz(path: &Path) {
        let enc = flate2::write::GzEncoder::new(
            File::create(path).unwrap(),
            flate2::Compression::default(),
        );
        let mut b = tar::Builder::new(enc);
        write_tar_entries(&mut b);
        b.finish().unwrap();
        b.into_inner().unwrap().finish().unwrap();
    }

    fn make_tar_xz(path: &Path) {
        let enc = xz2::write::XzEncoder::new(File::create(path).unwrap(), 6);
        let mut b = tar::Builder::new(enc);
        write_tar_entries(&mut b);
        b.finish().unwrap();
        b.into_inner().unwrap().finish().unwrap();
    }

    fn make_zip(path: &Path) {
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        let opts = SimpleFileOptions::default();
        for (name, data) in [("hello.txt", HELLO), ("sub/deep.txt", DEEP)] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    fn make_7z(path: &Path) {
        let mut w = sevenz_rust::SevenZWriter::create(path).unwrap();
        for (name, data) in [("hello.txt", HELLO), ("sub/deep.txt", DEEP)] {
            let mut entry = sevenz_rust::SevenZArchiveEntry::new();
            entry.name = name.into();
            entry.has_stream = true;
            w.push_archive_entry(entry, Some(data)).unwrap();
        }
        w.finish().unwrap();
    }

    /// Checks that the root and subfolder listings are correct for each format.
    fn assert_listing(path: &str) {
        let root = list_archive(path.into(), "".into(), None).unwrap();
        let mut names: Vec<_> = root.entries.iter().map(|e| e.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["hello.txt", "sub"], "Wurzel von {path}");

        let hello = root.entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert!(!hello.is_dir);
        assert_eq!(hello.size, HELLO.len() as u64, "Größe in {path}");
        let sub = root.entries.iter().find(|e| e.name == "sub").unwrap();
        assert!(sub.is_dir, "sub ist Ordner in {path}");

        let inner = list_archive(path.into(), "sub".into(), None).unwrap();
        let inner_names: Vec<_> = inner.entries.iter().map(|e| e.name.clone()).collect();
        assert_eq!(inner_names, vec!["deep.txt"], "Unterebene von {path}");
    }

    #[test]
    fn listing_all_formats() {
        let dir = tmp_dir("list");
        let cases: [ArchiveCase; 5] = [
            ("a.zip", make_zip),
            ("a.tar", make_tar),
            ("a.tar.gz", make_tar_gz),
            ("a.tar.xz", make_tar_xz),
            ("a.7z", make_7z),
        ];
        for (name, make) in cases {
            let path = dir.join(name);
            make(&path);
            assert_listing(path.to_str().unwrap());
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_format_is_error() {
        assert!(list_archive("/tmp/x.rar".into(), "".into(), None).is_err());
    }

    #[test]
    fn match_target_selects_within_base() {
        let mut targets = HashSet::new();
        targets.insert("sub".to_string());
        // Only entries under "sub/" for a non-empty target set.
        assert_eq!(
            match_target("sub/deep.txt", "", false, &targets),
            Some("sub/deep.txt".to_string())
        );
        assert_eq!(match_target("hello.txt", "", false, &targets), None);
        // Empty target set (all) via the caller flag: here all=true.
        assert_eq!(
            match_target("hello.txt", "", true, &targets),
            Some("hello.txt".to_string())
        );
        // With a base prefix, that prefix is stripped.
        let empty = HashSet::new();
        assert_eq!(
            match_target("sub/deep.txt", "sub/", true, &empty),
            Some("deep.txt".to_string())
        );
    }
}
