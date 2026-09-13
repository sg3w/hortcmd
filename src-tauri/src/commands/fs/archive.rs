// ============================================================
// Archive handling: browse, extract, pack.
// Supported for reading/extracting: ZIP (incl. AES/ZipCrypto
// password), tar, tar.gz, tar.xz, and 7z (incl. password).
// Packing still produces ZIP archives.
// Uses the transfer infrastructure (Prog, spawn_op, OpDone) from
// the file module and the listing types from the dir module.
// ============================================================

pub(crate) mod extract;
#[cfg(windows)]
mod private_windows;
pub(crate) mod safety;
mod staging;

use crate::errors::{code, AppError, AppResult, ErrorAction};
use extract::ExtractJob;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
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

/// Extracts selected entries (or the whole archive if `names` is empty).
fn run_extract(
    app: AppHandle,
    id: String,
    job: ExtractJob,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
) {
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

    let (ok, errors) =
        match safety::Limits::load().and_then(|limits| extract::run(job, &mut prog, limits)) {
            Ok(result) => result,
            Err(error) => (0, vec![ItemError::new("", error)]),
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
    use super::extract::{extract_7z, extract_tar, extract_zip, match_target, write_entry};
    use super::*;
    use std::collections::HashSet;
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
    fn preflight(
        path: &str,
        password: &Option<String>,
        limits: safety::Limits,
    ) -> AppResult<safety::Manifest> {
        extract::preflight(path, password, limits, &AtomicBool::new(false))
    }

    struct Quiet;
    impl super::super::transfer::progress::ProgressSink for Quiet {
        fn progress(&self, _: super::super::transfer::events::OpProgress) {}
    }

    fn progress() -> Prog {
        Prog {
            sink: Arc::new(Quiet),
            id: "test".into(),
            op: "extract".into(),
            files_total: 0,
            bytes_total: 0,
            files_done: 0,
            bytes_done: 0,
            last: Instant::now(),
            cancel: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn safe_extraction_round_trip_all_formats() {
        let root = tmp_dir("extract-safe");
        for (name, make) in [
            ("a.zip", make_zip as fn(&Path)),
            ("a.tar", make_tar),
            ("a.tar.gz", make_tar_gz),
            ("a.tar.xz", make_tar_xz),
            ("a.7z", make_7z),
        ] {
            let archive = root.join(name);
            make(&archive);
            let dest = root.join(format!("{name}-out"));
            fs::create_dir(&dest).unwrap();
            let mut stage = staging::Stage::new(&dest).unwrap();
            let mut prog = progress();
            let path = archive.to_str().unwrap();
            let result = match archive_kind(path).unwrap() {
                ArchiveKind::Zip => extract_zip(
                    path,
                    "",
                    safety::Limits::default(),
                    &HashSet::new(),
                    &mut stage,
                    &None,
                    &mut prog,
                ),
                ArchiveKind::SevenZ => extract_7z(
                    path,
                    "",
                    safety::Limits::default(),
                    &HashSet::new(),
                    &mut stage,
                    &None,
                    &mut prog,
                ),
                kind => extract_tar(
                    kind,
                    path,
                    "",
                    safety::Limits::default(),
                    &HashSet::new(),
                    &mut stage,
                    &mut prog,
                ),
            }
            .unwrap();
            assert!(result.1.is_empty(), "{name}");
            stage.commit(&prog.cancel).unwrap();
            stage.cleanup().unwrap();
            assert_eq!(fs::read(dest.join("hello.txt")).unwrap(), HELLO, "{name}");
            assert_eq!(fs::read(dest.join("sub/deep.txt")).unwrap(), DEEP, "{name}");
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zip_traversal_and_duplicates_fail_before_output() {
        for names in [
            ["safe", "../outside"],
            ["safe", "SAFE"],
            ["DIR/a", "dir/b"],
            ["safe", "C:/outside"],
        ] {
            let root = tmp_dir("malicious");
            let archive = root.join("input.zip");
            let mut zip = ZipWriter::new(File::create(&archive).unwrap());
            for name in names {
                zip.start_file(name, SimpleFileOptions::default()).unwrap();
                zip.write_all(b"x").unwrap();
            }
            zip.finish().unwrap();
            assert!(
                preflight(archive.to_str().unwrap(), &None, safety::Limits::default()).is_err()
            );
            assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
            fs::remove_dir_all(root).unwrap();
        }
    }
    fn hostile_tar(names: &[&str], link: bool) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for name in names {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            header.set_size(if link { 0 } else { 1 });
            if link {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_link_name("/outside").unwrap();
            }
            // Fixture construction deliberately bypasses Builder's safe path setter.
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            builder
                .append(&header, if link { &b""[..] } else { &b"x"[..] })
                .unwrap();
        }
        builder.into_inner().unwrap()
    }

    #[test]
    fn hostile_paths_and_links_are_rejected_in_tar_variants_and_sevenz() {
        let root = tmp_dir("hostile-corpus");
        for (names, link) in [
            (vec!["../outside"], false),
            (vec!["/outside"], false),
            (vec!["C:/outside"], false),
            (vec!["a\\b"], false),
            (vec!["a", "a"], false),
            (vec!["Dir/a", "dir/b"], false),
            (vec!["NUL.txt"], false),
            (vec!["link"], true),
        ] {
            let data = hostile_tar(&names, link);
            let tar_path = root.join("bad.tar");
            fs::write(&tar_path, &data).unwrap();
            let gz_path = root.join("bad.tar.gz");
            let mut gz = flate2::write::GzEncoder::new(
                File::create(&gz_path).unwrap(),
                flate2::Compression::default(),
            );
            gz.write_all(&data).unwrap();
            gz.finish().unwrap();
            let xz_path = root.join("bad.tar.xz");
            let mut xz = xz2::write::XzEncoder::new(File::create(&xz_path).unwrap(), 1);
            xz.write_all(&data).unwrap();
            xz.finish().unwrap();
            let seven_path = root.join("bad.7z");
            let mut seven = sevenz_rust::SevenZWriter::create(&seven_path).unwrap();
            for name in &names {
                let mut entry = sevenz_rust::SevenZArchiveEntry::new();
                entry.name = (*name).into();
                entry.has_stream = true;
                if link {
                    entry.has_windows_attributes = true;
                    entry.windows_attributes = 0x400;
                }
                seven.push_archive_entry(entry, Some(&b"x"[..])).unwrap();
            }
            seven.finish().unwrap();
            for path in [&tar_path, &gz_path, &xz_path, &seven_path] {
                assert!(
                    preflight(path.to_str().unwrap(), &None, safety::Limits::default()).is_err(),
                    "accepted {names:?} in {path:?}"
                );
            }
        }
        let zip_path = root.join("link.zip");
        let mut zip = ZipWriter::new(File::create(&zip_path).unwrap());
        zip.add_symlink("link", "/outside", SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        assert!(preflight(zip_path.to_str().unwrap(), &None, safety::Limits::default()).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_discards_staged_partial_output() {
        let root = tmp_dir("cancel-extract");
        fs::write(root.join("old"), b"original").unwrap();
        let mut stage = staging::Stage::new(&root).unwrap();
        let mut manifest = safety::Manifest::new(safety::Limits::default(), 100);
        manifest.add("old", false, 3, false).unwrap();
        let mut prog = progress();
        prog.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let error = write_entry(
            &mut &b"new"[..],
            &mut stage,
            "old",
            3,
            &mut prog,
            &mut manifest,
        )
        .unwrap_err();
        assert!(error.is(code::CANCELLED));
        stage.cleanup().unwrap();
        assert_eq!(fs::read(root.join("old")).unwrap(), b"original");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn shared_service_rejects_limits_without_touching_existing_output() {
        let root = tmp_dir("service-budget");
        let archive = root.join("a.zip");
        make_zip(&archive);
        let destination = root.join("out");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("hello.txt"), b"original").unwrap();
        let (ok, errors) = extract::run(
            ExtractJob {
                archive: archive.to_string_lossy().into_owned(),
                base_inner: String::new(),
                names: Vec::new(),
                dest_dir: destination.to_string_lossy().into_owned(),
                pw: None,
            },
            &mut progress(),
            safety::Limits {
                entry_bytes: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ok, 0);
        assert!(errors.iter().any(|e| e.error.is(code::ARCHIVE_LIMIT)));
        assert_eq!(
            fs::read(destination.join("hello.txt")).unwrap(),
            b"original"
        );
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancelled_preflight_does_not_open_the_archive() {
        let error = extract::preflight(
            "/not-an-archive",
            &None,
            safety::Limits::default(),
            &AtomicBool::new(true),
        )
        .err()
        .unwrap();
        assert!(error.is(code::CANCELLED));
    }
}
