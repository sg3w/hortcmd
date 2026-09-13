// ============================================================
// Archive handling: browse, extract, pack.
// Supported for reading/extracting: ZIP (incl. AES/ZipCrypto
// password), tar, tar.gz, tar.xz, and 7z (incl. password).
// Packing still produces ZIP archives.
// Uses the transfer infrastructure (Prog, spawn_op, OpDone) from
// the file module and the listing types from the dir module.
// ============================================================

pub(crate) mod extract;
pub(crate) mod pack;
#[cfg(windows)]
mod private_windows;
pub(crate) mod safety;
mod staging;

use super::transfer::recovery::{
    self,
    types::{Intent, RecoveryKind, RecoverySettings},
};
use crate::errors::{code, AppError, AppResult, ErrorAction};
use extract::ExtractJob;
use std::collections::{BTreeSet, HashSet};
use std::fs::File;
use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use zip::ZipArchive;

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
    read_archive(archive, inner, password)
}

pub(crate) fn read_archive(
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

    let run = recovery::optional_begin(&id, || {
        Intent::transfer(
            RecoveryKind::Extract,
            std::slice::from_ref(&job.archive),
            &job.dest_dir,
            RecoverySettings::default(),
        )
        .map(|mut intent| {
            intent.base = job.base_inner.clone();
            intent.names = job.names.clone();
            intent
        })
    });
    let (ok, mut errors) = match run.as_ref().map_err(Clone::clone).and_then(|run| {
        safety::Limits::load().and_then(|limits| {
            extract::run_with_recovery(job, &mut prog, limits, run.as_ref().map(|run| &run.0))
        })
    }) {
        Ok(result) => result,
        Err(error) => (0, vec![ItemError::new("", error)]),
    };

    let cancelled = prog.cancelled();
    if let Ok(Some(run)) = &run {
        let reason = if cancelled {
            Some(code::CANCELLED)
        } else {
            errors.first().map(|e| e.error.code.as_ref())
        };
        if let Err(error) = run.0.finish(reason) {
            errors.push(ItemError::new("", error));
        }
    }
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

/// GUI adapter for the shared, failure-safe ZIP service.
fn run_pack(
    app: AppHandle,
    id: String,
    sources: Vec<String>,
    dest_zip: String,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
) {
    let mut prog = Prog {
        sink: Arc::new(app.clone()),
        id: id.clone(),
        op: "pack".into(),
        files_total: 0,
        bytes_total: 0,
        files_done: 0,
        bytes_done: 0,
        last: Instant::now(),
        cancel,
        pause,
    };
    let run = recovery::optional_begin(&id, || {
        Intent::transfer(
            RecoveryKind::Pack,
            &sources,
            &dest_zip,
            RecoverySettings::default(),
        )
    });
    let (ok, mut errors) = match run.as_ref().map_err(Clone::clone).and_then(|run| {
        safety::Limits::load().and_then(|limits| {
            pack::run_with_recovery(
                &sources,
                &dest_zip,
                &mut prog,
                limits,
                &|_| Ok(()),
                run.as_ref().map(|run| &run.0),
            )
        })
    }) {
        Ok(count) => (count, Vec::new()),
        Err(error) => (0, vec![ItemError::new(&dest_zip, error)]),
    };
    if let Ok(Some(run)) = &run {
        let reason = if prog.cancelled() {
            Some(code::CANCELLED)
        } else {
            errors.first().map(|e| e.error.code.as_ref())
        };
        if let Err(error) = run.0.finish(reason) {
            errors.push(ItemError::new("", error));
        }
    }
    let _ = app.emit(
        "fs-done",
        OpDone {
            id,
            op: "pack".into(),
            ok,
            errors,
            cancelled: prog.cancelled(),
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
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use zip::{write::SimpleFileOptions, ZipWriter};

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
    fn shared_packing_preserves_empty_directories_and_finishes_a_valid_zip() {
        let root = tmp_dir("pack-roundtrip");
        let source = root.join("source");
        fs::create_dir_all(source.join("empty")).unwrap();
        fs::write(source.join("payload"), b"archive contents").unwrap();
        let target = root.join("result.zip");
        fs::write(&target, b"old archive").unwrap();
        assert_eq!(
            pack::run(
                &[source.to_string_lossy().into_owned()],
                target.to_str().unwrap(),
                &mut progress(),
                safety::Limits::default(),
                &|_| Ok(())
            )
            .unwrap(),
            1
        );
        let mut zip = ZipArchive::new(File::open(&target).unwrap()).unwrap();
        assert!(zip.by_name("source/empty/").unwrap().is_dir());
        let mut content = String::new();
        zip.by_name("source/payload")
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert_eq!(content, "archive contents");
        drop(zip);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancelling_inside_a_packed_file_preserves_the_previous_archive() {
        let root = tmp_dir("pack-cancel");
        let source = root.join("payload");
        fs::write(&source, vec![7u8; 512 * 1024]).unwrap();
        let target = root.join("result.zip");
        fs::write(&target, b"old archive").unwrap();
        let mut prog = progress();
        let cancel = prog.cancel.clone();
        let checks = std::cell::Cell::new(0);
        let result = pack::run(
            &[source.to_string_lossy().into_owned()],
            target.to_str().unwrap(),
            &mut prog,
            safety::Limits::default(),
            &|path| {
                if path.file_name().is_some_and(|name| name == "payload") {
                    checks.set(checks.get() + 1);
                    if checks.get() == 4 {
                        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(prog.bytes_done > 0 && prog.bytes_done < 512 * 1024);
        assert_eq!(fs::read(&target).unwrap(), b"old archive");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_packed_names_leave_the_destination_intact() {
        let root = tmp_dir("pack-duplicates");
        let source = root.join("payload");
        fs::write(&source, b"payload").unwrap();
        let target = root.join("result.zip");
        fs::write(&target, b"old archive").unwrap();
        let source = source.to_string_lossy().into_owned();
        assert!(pack::run(
            &[source.clone(), source],
            target.to_str().unwrap(),
            &mut progress(),
            safety::Limits::default(),
            &|_| Ok(())
        )
        .is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old archive");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn packing_refuses_directory_links_without_losing_the_old_archive() {
        let root = tmp_dir("pack-links");
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        std::os::unix::fs::symlink(&source, source.join("cycle")).unwrap();
        let target = root.join("result.zip");
        fs::write(&target, b"old archive").unwrap();
        assert!(pack::run(
            &[source.to_string_lossy().into_owned()],
            target.to_str().unwrap(),
            &mut progress(),
            safety::Limits::default(),
            &|_| Ok(())
        )
        .is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old archive");
        fs::remove_dir_all(root).unwrap();
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
            let manifest = preflight(path, &None, safety::Limits::default()).unwrap();
            let result = match archive_kind(path).unwrap() {
                ArchiveKind::Zip => extract_zip(
                    path,
                    "",
                    manifest,
                    &HashSet::new(),
                    &mut stage,
                    &None,
                    &mut prog,
                ),
                ArchiveKind::SevenZ => extract_7z(
                    path,
                    "",
                    manifest,
                    &HashSet::new(),
                    &mut stage,
                    &None,
                    &mut prog,
                ),
                kind => extract_tar(
                    kind,
                    path,
                    "",
                    manifest,
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
