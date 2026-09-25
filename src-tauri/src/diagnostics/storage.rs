use super::{safe_record, types::*};
use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
pub const FILE_BYTES: u64 = 512 * 1024;
pub const FILES: usize = 4;
pub const AGE: f64 = 7.0 * 86400.0;
pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
pub struct Store {
    pub root: PathBuf,
}
impl Store {
    pub fn prepare(&self) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        if fs::symlink_metadata(&self.root)?.file_type().is_symlink() {
            return Err(io::Error::other("linked diagnostic directory"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
    fn path(&self, i: usize) -> PathBuf {
        self.root.join(format!("events-{i}.jsonl"))
    }
    fn file(&self, i: usize, append: bool) -> io::Result<fs::File> {
        let path = self.path(i);
        if fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
            return Err(io::Error::other("not a regular log"));
        }
        let mut o = fs::OpenOptions::new();
        o.read(!append).append(append).create(append);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            o.custom_flags(0x00200000);
        }
        o.open(path)
    }
    pub fn cleanup(&self) -> io::Result<()> {
        for i in 0..FILES {
            let path = self.path(i);
            if let Ok(m) = fs::symlink_metadata(&path) {
                let old = m
                    .modified()
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|d| d.as_secs_f64() > AGE);
                if old || m.len() > FILE_BYTES {
                    fs::remove_file(path)?;
                }
            }
        }
        Ok(())
    }
    pub fn append(&self, record: DiagnosticRecord) -> io::Result<()> {
        let Some(record) = safe_record(record) else {
            return Ok(());
        };
        self.prepare()?;
        self.cleanup()?;
        let mut bytes = serde_json::to_vec(&record)?;
        bytes.push(b'\n');
        if self
            .file(0, false)
            .and_then(|f| f.metadata())
            .is_ok_and(|m| m.len() + bytes.len() as u64 > FILE_BYTES)
        {
            let _ = fs::remove_file(self.path(FILES - 1));
            for i in (0..FILES - 1).rev() {
                if self.path(i).exists() {
                    fs::rename(self.path(i), self.path(i + 1))?;
                }
            }
        }
        self.file(0, true)?.write_all(&bytes)
    }
    pub fn records(&self) -> Vec<DiagnosticRecord> {
        let _ = self.cleanup();
        let mut out = Vec::new();
        for i in (0..FILES).rev() {
            if let Ok(file) = self.file(i, false) {
                let mut data = String::new();
                if file.take(FILE_BYTES).read_to_string(&mut data).is_err() {
                    continue;
                }
                for line in data.lines() {
                    if let Some(record) = serde_json::from_str(line).ok().and_then(safe_record) {
                        if now() - record.at <= AGE && record.at <= now() + 60.0 {
                            out.push(record);
                        }
                    }
                }
            }
        }
        out
    }
    pub fn clear(&self) -> io::Result<()> {
        for i in 0..FILES {
            match fs::remove_file(self.path(i)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
