//! Extraction writes only inside a private directory held by an open handle.
//! Publication backs up each old file and rolls back completed renames on error.
use super::safety;
use crate::commands::fs::transfer::recovery::{
    self,
    types::{Item, Phase, RecoveryLocation},
    Handle, Step,
};
use crate::errors::{AppError, AppResult};
use cap_std::fs::{Dir, DirBuilder, File, OpenOptions};
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

pub struct Stage {
    recovery: Option<Handle>,
    committed: bool,
    recovery_root: Option<Step>,
    destination: PathBuf,
    root: Dir,
    data: Dir,
    backups: Dir,
    name: String,
    files: Vec<String>,
    directories: BTreeSet<String>,
    /// Failed rollback keeps backups for recovery; cleanup must not erase them.
    keep: bool,
}

impl Stage {
    #[cfg(test)]
    pub fn new(destination: &Path) -> AppResult<Self> {
        Self::new_with_recovery(destination, None)
    }

    pub fn new_with_recovery(destination: &Path, recovery: Option<&Handle>) -> AppResult<Self> {
        let root = Dir::open_ambient_dir(destination, cap_std::ambient_authority())?;
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| safety::unsafe_entry())?;
        let name = format!(".hortcmd-extract-{}", hex::encode(random));
        let location = RecoveryLocation::Local {
            path: destination.to_string_lossy().into_owned(),
        };
        let recovery_root = recovery
            .map(|handle| {
                handle.track(Item {
                    source: None,
                    target: location.clone(),
                    staging: Some(location.with_path(destination.join(&name))?),
                    backup: None,
                    phase: Phase::Writing,
                    digest: None,
                    durable: false,
                    owner: None,
                })
            })
            .transpose()?;
        let builder = DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use cap_std::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        root.create_dir_with(&name, &builder)?;
        if let Some(step) = &recovery_root {
            root.try_clone()?.into_std_file().sync_all()?;
            step.own(location.with_path(destination.join(&name))?)?;
            recovery::fault("staging");
        }
        let result = (|| -> io::Result<(Dir, Dir)> {
            #[cfg(windows)]
            super::private_windows::protect(&root, &name)?;
            let staging = root.open_dir(&name)?;
            staging.create_dir("data")?;
            staging.create_dir("backups")?;
            Ok((staging.open_dir("data")?, staging.open_dir("backups")?))
        })();
        match result {
            Ok((data, backups)) => Ok(Self {
                recovery: recovery.cloned(),
                committed: false,
                recovery_root,
                destination: destination.to_path_buf(),
                root,
                data,
                backups,
                name,
                files: Vec::new(),
                directories: BTreeSet::new(),
                keep: false,
            }),
            Err(error) => {
                root.remove_dir_all(&name)?;
                Err(error.into())
            }
        }
    }

    fn parents(&mut self, path: &str) -> AppResult<()> {
        if let Some(parent) = Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            let mut prefix = PathBuf::new();
            for part in parent.components() {
                prefix.push(part);
                self.directories
                    .insert(prefix.to_string_lossy().into_owned());
            }
            self.data.create_dir_all(parent)?;
        }
        Ok(())
    }

    pub fn directory(&mut self, name: &str, limits: &safety::Limits) -> AppResult<()> {
        let name = safety::path(name, true, limits)?;
        self.parents(&name)?;
        self.data.create_dir_all(&name)?;
        self.directories.insert(name);
        Ok(())
    }

    pub fn file(&mut self, name: &str, limits: &safety::Limits) -> AppResult<File> {
        let name = safety::path(name, false, limits)?;
        self.parents(&name)?;
        let file = self
            .data
            .open_with(&name, OpenOptions::new().write(true).create_new(true))?;
        self.files.push(name);
        Ok(file)
    }

    fn check_destination(&self, name: &str, directory: bool) -> AppResult<()> {
        let mut path = PathBuf::new();
        let parts: Vec<_> = Path::new(name).components().collect();
        for (i, component) in parts.iter().enumerate() {
            path.push(component);
            match self.root.symlink_metadata(&path) {
                Ok(meta)
                    if meta.file_type().is_symlink()
                        || meta.is_dir() != (i + 1 < parts.len() || directory) =>
                {
                    return Err(safety::unsafe_entry())
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    pub fn commit(&mut self, cancel: &std::sync::atomic::AtomicBool) -> AppResult<()> {
        // Check every target before moving the first old file aside.
        for name in &self.directories {
            self.check_destination(name, true)?;
        }
        for name in &self.files {
            self.check_destination(name, false)?;
        }
        let mut created = Vec::new();
        let mut published: Vec<(String, bool, bool, Option<Step>)> = Vec::new();
        let result = (|| -> AppResult<()> {
            for name in &self.directories {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err(AppError::new(crate::errors::code::CANCELLED));
                }
                match self.root.create_dir(name) {
                    Ok(()) => created.push(name),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                        self.check_destination(name, true)?
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            for (i, name) in self.files.iter().enumerate() {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err(AppError::new(crate::errors::code::CANCELLED));
                }
                self.check_destination(name, false)?;
                let step = self
                    .recovery
                    .as_ref()
                    .map(|handle| {
                        let location = RecoveryLocation::Local {
                            path: self.destination.to_string_lossy().into_owned(),
                        };
                        let staging = self.destination.join(&self.name).join("data").join(name);
                        handle.track(Item {
                            source: None,
                            target: location.with_path(self.destination.join(name))?,
                            staging: Some(location.with_path(&staging)?),
                            backup: Some(
                                location.with_path(
                                    self.destination
                                        .join(&self.name)
                                        .join("backups")
                                        .join(i.to_string()),
                                )?,
                            ),
                            phase: Phase::Prepared,
                            digest: Some(recovery::durability::fingerprint_cancellable(
                                &staging, true, cancel,
                            )?),
                            durable: true,
                            owner: None,
                        })
                    })
                    .transpose()?;
                if let Some(step) = &step {
                    recovery::fault("prepared");
                    step.transition(Phase::Publishing)?;
                }
                let backup = match self.root.rename(name, &self.backups, i.to_string()) {
                    Ok(()) => true,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => false,
                    Err(e) => return Err(e.into()),
                };
                published.push((name.to_owned(), backup, false, step.clone()));
                if self.recovery.is_some() {
                    self.backups.try_clone()?.into_std_file().sync_all()?;
                    self.root.try_clone()?.into_std_file().sync_all()?;
                }
                recovery::fault("backup");
                self.data.rename(name, &self.root, name)?;
                published.last_mut().unwrap().2 = true;
                recovery::fault("swapped");
                if let Some(step) = &step {
                    recovery::durability::sync_parent(&self.destination.join(name))?;
                    self.backups.try_clone()?.into_std_file().sync_all()?;
                    self.data.try_clone()?.into_std_file().sync_all()?;
                    step.transition(Phase::Committed)?;
                    recovery::fault("committed");
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            let mut incomplete = false;
            for (i, (name, backup, installed, _)) in published.iter().enumerate().rev() {
                if *installed && self.root.remove_file(name).is_err() {
                    incomplete = true;
                }
                if *backup
                    && self
                        .backups
                        .rename(i.to_string(), &self.root, name)
                        .is_err()
                {
                    incomplete = true;
                }
            }
            for name in created.iter().rev() {
                if self.root.remove_dir(name).is_err() {
                    incomplete = true;
                }
            }
            // Keep recovery evidence until rollback and its journal updates are durable.
            self.keep = true;
            if !incomplete {
                if self.recovery.is_some() {
                    self.root.try_clone()?.into_std_file().sync_all()?;
                    self.backups.try_clone()?.into_std_file().sync_all()?;
                }
                for (_, _, _, step) in &published {
                    if let Some(step) = step {
                        step.transition(Phase::Skipped)?;
                    }
                }
                self.keep = false;
            }
            if self.keep {
                return Err(AppError::new(crate::errors::code::TRANSFER_COMMIT)
                    .op("extract-rollback")
                    .detail(format!(
                        "Rollback incomplete; recovery data remains in {}",
                        self.name
                    )));
            }
            return Err(error);
        }
        if let Some(step) = &self.recovery_root {
            for name in self.directories.iter().rev() {
                self.root.open_dir(name)?.into_std_file().sync_all()?;
            }
            self.root.try_clone()?.into_std_file().sync_all()?;
            step.transition(Phase::Committed)?;
        }
        self.committed = true;
        Ok(())
    }

    /// Always called explicitly; a failed cleanup becomes a visible error.
    pub fn cleanup(&self) -> AppResult<()> {
        if !self.keep {
            self.root.remove_dir_all(&self.name).map_err(|error| {
                AppError::from(error)
                    .op("extract-cleanup")
                    .detail(format!("Staging cleanup failed; inspect {}", self.name))
            })?;
            if let Some(step) = &self.recovery_root {
                self.root.try_clone()?.into_std_file().sync_all()?;
                step.transition(if self.committed {
                    Phase::Committed
                } else {
                    Phase::Skipped
                })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp() -> PathBuf {
        let path = std::env::temp_dir().join(format!("hortcmd-stage-{}", crate::net::unique_id()));
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn preserves_old_files_until_commit_and_keeps_empty_directories() {
        let path = temp();
        std::fs::write(path.join("a"), "old").unwrap();
        let mut stage = Stage::new(&path).unwrap();
        stage
            .file("a", &safety::Limits::default())
            .unwrap()
            .write_all(b"new")
            .unwrap();
        stage
            .directory("empty/", &safety::Limits::default())
            .unwrap();
        assert_eq!(std::fs::read_to_string(path.join("a")).unwrap(), "old");
        stage
            .commit(&std::sync::atomic::AtomicBool::new(false))
            .unwrap();
        stage.cleanup().unwrap();
        assert_eq!(std::fs::read_to_string(path.join("a")).unwrap(), "new");
        assert!(path.join("empty").is_dir());
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 2);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn a_late_conflict_leaves_every_old_destination_intact() {
        let path = temp();
        std::fs::write(path.join("a"), "old").unwrap();
        let mut stage = Stage::new(&path).unwrap();
        for name in ["a", "b"] {
            stage
                .file(name, &safety::Limits::default())
                .unwrap()
                .write_all(b"new")
                .unwrap();
        }
        std::fs::create_dir(path.join("b")).unwrap();
        assert!(stage
            .commit(&std::sync::atomic::AtomicBool::new(false))
            .is_err());
        stage.cleanup().unwrap();
        assert_eq!(std::fs::read_to_string(path.join("a")).unwrap(), "old");
        assert!(path.join("b").is_dir());
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn a_publish_failure_rolls_back_files_already_replaced() {
        let path = temp();
        for name in ["a", "b"] {
            std::fs::write(path.join(name), format!("old-{name}")).unwrap();
        }
        let mut stage = Stage::new(&path).unwrap();
        for name in ["a", "b"] {
            stage
                .file(name, &safety::Limits::default())
                .unwrap()
                .write_all(b"new")
                .unwrap();
        }
        // A deterministic I/O failure after a has been replaced and b backed up.
        stage.data.remove_file("b").unwrap();
        assert!(stage
            .commit(&std::sync::atomic::AtomicBool::new(false))
            .is_err());
        stage.cleanup().unwrap();
        for name in ["a", "b"] {
            assert_eq!(
                std::fs::read_to_string(path.join(name)).unwrap(),
                format!("old-{name}")
            );
        }
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 2);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn destination_directory_substitution_cannot_escape_root() {
        let path = temp();
        let outside = temp();
        std::fs::write(outside.join("secret"), "original").unwrap();
        std::fs::create_dir(path.join("sub")).unwrap();
        let mut stage = Stage::new(&path).unwrap();
        stage
            .file("sub/secret", &safety::Limits::default())
            .unwrap()
            .write_all(b"replacement")
            .unwrap();
        std::fs::remove_dir(path.join("sub")).unwrap();
        std::os::unix::fs::symlink(&outside, path.join("sub")).unwrap();
        assert!(stage
            .commit(&std::sync::atomic::AtomicBool::new(false))
            .is_err());
        stage.cleanup().unwrap();
        assert_eq!(
            std::fs::read_to_string(outside.join("secret")).unwrap(),
            "original"
        );
        std::fs::remove_dir_all(path).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn abandoning_a_stage_changes_no_existing_files() {
        let path = temp();
        std::fs::write(path.join("a"), "old").unwrap();
        let mut stage = Stage::new(&path).unwrap();
        stage
            .file("a", &safety::Limits::default())
            .unwrap()
            .write_all(b"partial")
            .unwrap();
        stage.cleanup().unwrap();
        assert_eq!(std::fs::read_to_string(path.join("a")).unwrap(), "old");
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
        std::fs::remove_dir_all(path).unwrap();
    }
}
