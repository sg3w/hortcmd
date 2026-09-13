//! Private atomic snapshots. One process owns the journal; readers never repair it.
use super::types::Record;
use cap_std::fs::{Dir, OpenOptions};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

pub const MAX_RECORD_BYTES: usize = 1024 * 1024;
pub const MAX_RECORDS: usize = 128;
pub const MAX_ITEMS: usize = 20_000;

pub struct Storage {
    root: Dir,
    _lock: File,
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid transfer journal")
}
fn file_name(id: &str) -> String {
    format!("{}.json", hex::encode(Sha256::digest(id.as_bytes())))
}

impl Storage {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut missing = Vec::new();
        let mut current = path;
        while !current.try_exists()? {
            missing.push(current);
            current = current
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .ok_or_else(invalid)?;
        }
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)?;
        // Persist every newly introduced directory entry before accepting intent.
        for created in missing.iter().rev() {
            File::open(created.parent().ok_or_else(invalid)?)?.sync_all()?;
        }
        if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(invalid());
        }
        let root = Dir::open_ambient_dir(path, cap_std::ambient_authority())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            root.set_permissions(
                ".",
                cap_std::fs::Permissions::from_std(std::fs::Permissions::from_mode(0o700)),
            )?;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = root.open_with("lock", &options)?.into_std();
        lock.try_lock_exclusive()?;
        Ok(Self { root, _lock: lock })
    }

    fn sync_root(&self) -> io::Result<()> {
        self.root.try_clone()?.into_std_file().sync_all()
    }

    pub fn load(&self) -> io::Result<Vec<Record>> {
        let mut records = Vec::new();
        let mut entries = 0;
        for entry in self.root.entries()? {
            let entry = entry?;
            entries += 1;
            if entries > MAX_RECORDS * 3 {
                return Err(invalid());
            }
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(invalid)?;
            if name == "lock" {
                continue;
            }
            // Interrupted atomic writes have no execution authority. Only remove
            // private journal temporaries, never transfer artifacts.
            if name.ends_with(".tmp") {
                if entry.file_type()?.is_file() {
                    self.root.remove_file(name)?;
                }
                continue;
            }
            if !name.ends_with(".json") {
                return Err(invalid());
            }
            if !entry.file_type()?.is_file() || entry.metadata()?.len() > MAX_RECORD_BYTES as u64 {
                return Err(invalid());
            }
            if records.len() >= MAX_RECORDS {
                return Err(invalid());
            }
            let mut bytes = Vec::new();
            self.root
                .open(name)?
                .take(MAX_RECORD_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_RECORD_BYTES {
                return Err(invalid());
            }
            let record: Record = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
            if record.version != 1
                || record.items.len() > MAX_ITEMS
                || file_name(&record.id) != name
            {
                return Err(invalid());
            }
            records.push(record);
        }
        records.sort_by_key(|record| record.order);
        Ok(records)
    }

    pub fn save(&self, record: &Record) -> io::Result<()> {
        if record.version != 1 || record.items.len() > MAX_ITEMS {
            return Err(invalid());
        }
        let bytes = serde_json::to_vec(record).map_err(|_| invalid())?;
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(invalid());
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|_| io::Error::other("journal randomness unavailable"))?;
        let temporary = format!("{}.tmp", hex::encode(random));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = self.root.open_with(&temporary, &options)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            self.root
                .rename(&temporary, &self.root, file_name(&record.id))?;
            self.sync_root()
        })();
        if result.is_err() {
            let _ = self.root.remove_file(&temporary);
        }
        result
    }

    pub fn remove(&self, id: &str) -> io::Result<()> {
        self.root.remove_file(file_name(id))?;
        self.sync_root()
    }
}
