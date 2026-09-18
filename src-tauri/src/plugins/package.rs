//! Bounded local package staging; archives never extract outside the staging root.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_FILES: usize = 4096;
fn files(root: &Path, relative: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if relative.components().count() > 16 {
        return Err("Plugin directory nesting exceeds 16".into());
    }
    for entry in fs::read_dir(root.join(relative)).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = relative.join(entry.file_name());
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if out.len() >= MAX_FILES {
            return Err("Plugin package exceeds 4096 entries".into());
        }
        out.push(path.clone());
        if kind.is_dir() {
            files(root, &path, out)?;
        } else if !kind.is_file() {
            return Err("Plugin packages cannot contain links or special files".into());
        }
    }
    Ok(())
}
pub fn fingerprint(root: &Path) -> Result<String, String> {
    if !fs::symlink_metadata(root)
        .map_err(|e| e.to_string())?
        .is_dir()
    {
        return Err("Plugin root must be a directory".into());
    }
    let mut paths = vec![];
    files(root, Path::new(""), &mut paths)?;
    paths.sort();
    let mut hash = Sha256::new();
    let mut remaining = MAX_BYTES;
    for path in paths {
        let name = path.to_string_lossy();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        let full = root.join(&path);
        let meta = fs::symlink_metadata(&full).map_err(|e| e.to_string())?;
        hash.update([u8::from(meta.is_dir())]);
        if meta.is_file() {
            let mut bytes = Vec::new();
            fs::File::open(full)
                .map_err(|e| e.to_string())?
                .take(remaining + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            remaining = remaining
                .checked_sub(bytes.len() as u64)
                .ok_or("Plugin package exceeds 256 MiB")?;
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn copy_file(mut source: impl Read, target: &Path, remaining: &mut u64) -> Result<(), String> {
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|e| e.to_string())?;
    let count = std::io::copy(&mut source.by_ref().take(*remaining + 1), &mut output)
        .map_err(|e| e.to_string())?;
    *remaining = remaining
        .checked_sub(count)
        .ok_or("Plugin package exceeds 256 MiB")?;
    output.flush().map_err(|e| e.to_string())
}
pub fn stage(source: &Path, target: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(source).map_err(|e| e.to_string())?;
    let mut remaining = MAX_BYTES;
    if meta.is_dir() {
        let source = source.canonicalize().map_err(|e| e.to_string())?;
        let destination = target.canonicalize().map_err(|e| e.to_string())?;
        if destination.starts_with(&source) {
            return Err("Plugin source cannot contain the installation directory".into());
        }
        let mut paths = vec![];
        files(&source, Path::new(""), &mut paths)?;
        for path in paths {
            let full = source.join(&path);
            if full.is_dir() {
                fs::create_dir_all(target.join(path)).map_err(|e| e.to_string())?;
            } else {
                copy_file(
                    fs::File::open(full).map_err(|e| e.to_string())?,
                    &target.join(path),
                    &mut remaining,
                )?;
            }
        }
    } else if meta.is_file() {
        let mut archive = zip::ZipArchive::new(fs::File::open(source).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if archive.len() > MAX_FILES {
            return Err("Plugin archive exceeds 4096 entries".into());
        }
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
            let path = PathBuf::from(entry.name());
            if entry.name().contains(['\\', ':'])
                || path.components().count() > 16
                || !path.components().all(|c| matches!(c, Component::Normal(_)))
                || path.as_os_str().is_empty()
            {
                return Err("Unsafe plugin archive path".into());
            }
            let mode = entry.unix_mode().unwrap_or(0) & 0o170000;
            if ![0, 0o100000, 0o040000].contains(&mode) {
                return Err("Plugin archives cannot contain links or special files".into());
            }
            let output = target.join(path);
            if entry.is_dir() {
                fs::create_dir_all(output).map_err(|e| e.to_string())?;
            } else {
                fs::create_dir_all(output.parent().ok_or("Missing archive parent")?)
                    .map_err(|e| e.to_string())?;
                copy_file(&mut entry, &output, &mut remaining)?;
            }
        }
    } else {
        return Err("Choose a local directory or ZIP file, not a link".into());
    }
    super::registry::read_manifest(target)?;
    Ok(())
}

pub struct Staging(PathBuf);
impl Staging {
    pub fn new(root: &Path) -> Result<Self, String> {
        let path = root.join(format!(".staging-{}", crate::net::unique_id()));
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
