// ============================================================
// Archiv-Handling: browsen, entpacken, packen.
// Unterstützt beim Lesen/Entpacken: ZIP (inkl. AES/ZipCrypto-
// Passwort), tar, tar.gz, tar.xz sowie 7z (inkl. Passwort).
// Packen erzeugt weiterhin ZIP-Archive.
// Nutzt die Transfer-Infrastruktur (Prog, spawn_op, OpDone) aus
// dem file-Modul und die Listen-Typen aus dem dir-Modul.
// ============================================================

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

use super::dir::{DirEntry, DirListing};
use super::file::{spawn_op, OpDone, Prog};

/// Sentinel-Fehler: Archiv/Eintrag ist verschlüsselt, aber es wurde
/// kein Passwort übergeben. Das Frontend erkennt dies und fragt nach.
const PW_REQUIRED: &str = "PASSWORD_REQUIRED";
/// Sentinel-Fehler: Das übergebene Passwort ist falsch.
const PW_WRONG: &str = "PASSWORD_WRONG";

/// Unterstützte Archivformate (per Dateiendung erkannt).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    Tar,
    TarGz,
    TarXz,
    SevenZ,
}

/// Erkennt das Archivformat anhand der (Doppel-)Endung.
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

/// Rohes Archivmitglied: Eintragsname (mit „/"-Trennern), Größe, Ordner-Flag.
type RawEntry = (String, u64, bool);

/// Normalisiert einen Innen-Pfad zu einem Präfix ("" oder "sub/").
fn inner_prefix(inner: &str) -> String {
    let trimmed = inner.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{}/", trimmed)
    }
}

/// Wandelt `sevenz_rust::Password` aus einem optionalen String.
fn password(opt: &Option<String>) -> sevenz_rust::Password {
    match opt {
        Some(s) => sevenz_rust::Password::from(s.as_str()),
        None => sevenz_rust::Password::empty(),
    }
}

/// Übersetzt 7z-Fehler in die Passwort-Sentinels bzw. Klartext.
fn map_7z_err(e: sevenz_rust::Error) -> String {
    match e {
        sevenz_rust::Error::PasswordRequired => PW_REQUIRED.to_string(),
        sevenz_rust::Error::MaybeBadPassword(_) => PW_WRONG.to_string(),
        other => other.to_string(),
    }
}

/// Öffnet einen streamenden Reader für ein tar-Archiv (ggf. dekomprimierend).
fn open_tar_reader(kind: ArchiveKind, archive: &str) -> Result<Box<dyn Read>, String> {
    let f = File::open(archive).map_err(|e| e.to_string())?;
    Ok(match kind {
        ArchiveKind::Tar => Box::new(f),
        ArchiveKind::TarGz => Box::new(flate2::read::GzDecoder::new(f)),
        ArchiveKind::TarXz => Box::new(xz2::read::XzDecoder::new(f)),
        _ => unreachable!("open_tar_reader nur für tar-Varianten"),
    })
}

// ---------- Auflisten ----------

/// Liest die Mitgliederliste eines ZIP-Archivs (nur Metadaten, ohne Passwort).
fn zip_raw(archive: &str) -> Result<Vec<RawEntry>, String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        // by_index_raw liest Metadaten auch bei verschlüsselten Einträgen.
        let e = zip.by_index_raw(i).map_err(|e| e.to_string())?;
        out.push((e.name().to_string(), e.size(), e.is_dir()));
    }
    Ok(out)
}

/// Liest die Mitgliederliste eines tar-Archivs (Header, ohne Datenströme).
fn tar_raw(reader: Box<dyn Read>) -> Result<Vec<RawEntry>, String> {
    let mut ar = tar::Archive::new(reader);
    let mut out = Vec::new();
    for entry in ar.entries().map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let is_dir = entry.header().entry_type().is_dir();
        let name = entry
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let size = entry.header().size().unwrap_or(0);
        out.push((name, size, is_dir));
    }
    Ok(out)
}

/// Liest die Mitgliederliste eines 7z-Archivs (ggf. mit Passwort).
fn sevenz_raw(archive: &str, pw: &Option<String>) -> Result<Vec<RawEntry>, String> {
    let ar = sevenz_rust::Archive::open_with_password(archive, &password(pw)).map_err(map_7z_err)?;
    Ok(ar
        .files
        .iter()
        .map(|f| (f.name.replace('\\', "/"), f.size, f.is_directory))
        .collect())
}

/// Baut aus der rohen Mitgliederliste die Ansicht der Ebene `inner`.
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
            // Untereintrag → oberste Ebene ist ein Ordner.
            Some(idx) => {
                dirs.insert(rest[..idx].to_string());
            }
            None if *is_dir => {
                dirs.insert(rest.to_string());
            }
            None => {
                if seen.insert(rest.to_string()) {
                    files.push(DirEntry {
                        name: rest.to_string(),
                        is_dir: false,
                        is_symlink: false,
                        size: *size,
                        modified: 0,
                        mode: None,
                    });
                }
            }
        }
    }
    for d in dirs {
        files.push(DirEntry {
            name: d,
            is_dir: true,
            is_symlink: false,
            size: 0,
            modified: 0,
            mode: None,
        });
    }

    DirListing {
        path: format!("/{}", inner.trim_matches('/')),
        entries: files,
    }
}

/// Listet den Inhalt eines Archivs auf der Ebene `inner`.
#[tauri::command]
pub fn list_archive(
    archive: String,
    inner: String,
    password: Option<String>,
) -> Result<DirListing, String> {
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

// ---------- Entpacken ----------

/// Prüft, ob ein Eintrag zu entpacken ist, und liefert seinen Zielpfad
/// relativ zur gewählten Ebene (`base`).
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

/// Schreibt einen Datenstrom als Datei nach `out` und meldet Fortschritt.
fn write_entry(
    reader: &mut dyn Read,
    out: &Path,
    name: &str,
    file_total: u64,
    prog: &mut Prog,
) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut f = File::create(out).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut file_done = 0u64;
    prog.emit(name, 0, file_total, true);
    loop {
        if prog.cancelled() {
            break;
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        file_done += n as u64;
        prog.bytes_done += n as u64;
        prog.emit(name, file_done, file_total, false);
    }
    prog.files_done += 1;
    prog.emit(name, file_total, file_total, true);
    Ok(())
}

/// Entpackt passende Einträge aus einem ZIP-Archiv.
fn extract_zip(
    archive: &str,
    base: &str,
    all: bool,
    targets: &HashSet<String>,
    dest: &Path,
    pw: &Option<String>,
    prog: &mut Prog,
) -> Result<(u32, Vec<String>), String> {
    let open = || -> Result<ZipArchive<File>, String> {
        ZipArchive::new(File::open(archive).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    };

    // Passage 1: passende Indizes + Summen ermitteln, Verschlüsselung prüfen.
    let mut zip = open()?;
    let mut indices = Vec::new();
    let mut needs_pw = false;
    for i in 0..zip.len() {
        let e = zip.by_index_raw(i).map_err(|e| e.to_string())?;
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
        return Err(PW_REQUIRED.to_string());
    }
    prog.emit("", 0, 0, true);

    // Passage 2: entpacken.
    let mut zip = open()?;
    let mut ok = 0u32;
    let mut errors = Vec::new();
    for i in indices {
        if prog.cancelled() {
            break;
        }
        match extract_zip_one(&mut zip, i, base, dest, pw, prog) {
            Ok(()) => ok += 1,
            // Falsches Passwort ist fatal → sofort abbrechen und melden.
            Err(e) if e == PW_WRONG || e == PW_REQUIRED => return Err(e),
            Err(e) => errors.push(e),
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
) -> Result<(), String> {
    // Metadaten in eigenem Scope lesen (getrennter Borrow).
    let (name, encrypted, size) = {
        let e = zip.by_index_raw(index).map_err(|e| e.to_string())?;
        (e.name().to_string(), e.encrypted(), e.size())
    };
    let rel = &name[base.len()..];
    let out = dest.join(rel);
    if encrypted {
        let pw = pw.as_deref().ok_or_else(|| PW_REQUIRED.to_string())?;
        let mut e = zip
            .by_index_decrypt(index, pw.as_bytes())
            .map_err(|err| match err {
                ZipError::InvalidPassword => PW_WRONG.to_string(),
                o => o.to_string(),
            })?;
        write_entry(&mut e, &out, &name, size, prog)
    } else {
        let mut e = zip.by_index(index).map_err(|e| e.to_string())?;
        write_entry(&mut e, &out, &name, size, prog)
    }
}

/// Entpackt passende Einträge aus einem tar-Archiv (ggf. dekomprimierend).
fn extract_tar(
    kind: ArchiveKind,
    archive: &str,
    base: &str,
    all: bool,
    targets: &HashSet<String>,
    dest: &Path,
    prog: &mut Prog,
) -> Result<(u32, Vec<String>), String> {
    // Passage 1: Summen aus den Headern.
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

    // Passage 2: entpacken (Reader neu öffnen, tar-Streams sind nicht seekbar).
    let mut ar = tar::Archive::new(open_tar_reader(kind, archive)?);
    let mut ok = 0u32;
    let mut errors = Vec::new();
    for entry in ar.entries().map_err(|e| e.to_string())? {
        if prog.cancelled() {
            break;
        }
        let mut entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let name = match entry.path() {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(e) => {
                errors.push(e.to_string());
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
            Err(e) => errors.push(e),
        }
    }
    Ok((ok, errors))
}

/// Entpackt passende Einträge aus einem 7z-Archiv (ggf. mit Passwort).
fn extract_7z(
    archive: &str,
    base: &str,
    all: bool,
    targets: &HashSet<String>,
    dest: &Path,
    pw: &Option<String>,
    prog: &mut Prog,
) -> Result<(u32, Vec<String>), String> {
    // Passage 1: Summen aus den Metadaten (öffnet zugleich mit Passwort).
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

    // Passage 2: entpacken über den streamenden Reader.
    let mut reader =
        sevenz_rust::SevenZReader::open(archive, password(pw)).map_err(map_7z_err)?;
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
                Err(e) => errors.push(e),
            }
            Ok(true)
        })
        .map_err(map_7z_err)?;
    Ok((ok, errors))
}

/// Entpackt ausgewählte Einträge (oder das ganze Archiv, wenn `names` leer ist).
fn run_extract(
    app: AppHandle,
    id: String,
    archive: String,
    base_inner: String,
    names: Vec<String>,
    dest_dir: String,
    pw: Option<String>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
) {
    let dest = PathBuf::from(&dest_dir);
    let base = inner_prefix(&base_inner);
    let targets: HashSet<String> = names.into_iter().collect();
    let all = targets.is_empty();

    let mut prog = Prog {
        app: app.clone(),
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
        None => Err("Unbekanntes Archivformat".to_string()),
    };

    let (ok, errors) = match result {
        Ok((ok, errors)) => (ok, errors),
        Err(e) => (0, vec![e]),
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
        },
    );
}

// ---------- Packen (ZIP) ----------

/// Sammelt alle Dateien unter `path` mit ihrem ZIP-Eintragsnamen.
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

/// Packt mehrere Quellen in ein neues ZIP-Archiv.
fn run_pack(
    app: AppHandle,
    id: String,
    sources: Vec<String>,
    dest_zip: String,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
) {
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
        app: app.clone(),
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
                    Err(e) => errors.push(e),
                }
            }
            if let Err(e) = zip.finish() {
                errors.push(e.to_string());
            }
        }
        Err(e) => errors.push(e.to_string()),
    }

    // Bei Abbruch das unvollständige Archiv entfernen.
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
        },
    );
}

fn pack_one(
    zip: &mut ZipWriter<File>,
    path: &Path,
    entry_name: &str,
    opts: SimpleFileOptions,
    prog: &mut Prog,
) -> Result<(), String> {
    zip.start_file(entry_name, opts).map_err(|e| e.to_string())?;
    let file_total = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut file_done = 0u64;
    let name = path.to_string_lossy().into_owned();
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        zip.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        file_done += n as u64;
        prog.bytes_done += n as u64;
        prog.emit(&name, file_done, file_total, false);
    }
    prog.files_done += 1;
    prog.emit(&name, file_total, file_total, true);
    Ok(())
}

// ---------- Commands ----------

/// Startet das Entpacken im Hintergrund.
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
    spawn_op(id, move |cancel, pause| {
        run_extract(app, id2, archive, base, names, dest_dir, password, cancel, pause)
    });
}

/// Startet das Packen im Hintergrund.
#[tauri::command]
pub fn create_archive(app: AppHandle, id: String, sources: Vec<String>, dest_zip: String) {
    let id2 = id.clone();
    spawn_op(id, move |cancel, pause| run_pack(app, id2, sources, dest_zip, cancel, pause));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // Erwarteter Inhalt der Testarchive: eine Datei in der Wurzel und
    // eine in einem Unterordner.
    const HELLO: &[u8] = b"Hallo Welt";
    const DEEP: &[u8] = b"tief verschachtelt";

    /// Eindeutiges temporäres Arbeitsverzeichnis für einen Test.
    fn tmp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rc_arch_{}_{}", tag, nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Schreibt beide Testdateien als tar-Einträge in einen Writer.
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

    /// Prüft, dass Wurzel- und Unterordner-Auflistung für jedes Format stimmen.
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
        let cases: [(&str, fn(&Path)); 5] = [
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
        // Nur Einträge unter "sub/" bei nicht-leerer Zielmenge.
        assert_eq!(
            match_target("sub/deep.txt", "", false, &targets),
            Some("sub/deep.txt".to_string())
        );
        assert_eq!(match_target("hello.txt", "", false, &targets), None);
        // Leere Zielmenge (all) via Aufrufer-Flag: hier all=true.
        assert_eq!(
            match_target("hello.txt", "", true, &targets),
            Some("hello.txt".to_string())
        );
        // Mit Basis-Präfix wird dieses abgeschnitten.
        let empty = HashSet::new();
        assert_eq!(
            match_target("sub/deep.txt", "sub/", true, &empty),
            Some("deep.txt".to_string())
        );
    }
}
