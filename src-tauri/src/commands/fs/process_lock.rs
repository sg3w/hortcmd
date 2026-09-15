//! An exclusive file lock owned by a Rust scope, not by inherited descriptors.
use fs2::FileExt;
use std::{fs::File, io};

pub(crate) struct ProcessLock(File);

impl ProcessLock {
    pub(crate) fn acquire(file: File) -> io::Result<Self> {
        file.try_lock_exclusive()?;
        Ok(Self(file))
    }
}

impl Drop for ProcessLock {
    fn drop(&mut self) {
        // On Unix, fork/dup shares the open-file description. close-on-exec does
        // not help between fork and exec: closing only our descriptor can leave
        // the lock held by a child. Unlock the description when its owner ends.
        // Closing the File remains the fallback if the OS rejects unlocking.
        let _ = FileExt::unlock(&self.0);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};

    #[test]
    fn inherited_descriptor_cannot_extend_or_release_a_new_owners_lock() {
        let path = std::env::temp_dir().join(format!(
            "hortcmd-process-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let open = || {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap()
        };
        let owner = ProcessLock::acquire(file).unwrap();
        // dup and fork share the same open-file description. Keep that description
        // alive deterministically instead of depending on a fork/exec timing race.
        let inherited = owner.0.try_clone().unwrap();
        assert!(ProcessLock::acquire(open()).is_err());
        // A failed acquisition must not unlock the actual owner's descriptor.
        assert!(ProcessLock::acquire(open()).is_err());
        drop(owner);
        let replacement = ProcessLock::acquire(open()).expect(
            "owner release must permit immediate reopen even while a child holds the descriptor",
        );
        assert!(ProcessLock::acquire(open()).is_err());
        drop(inherited);
        assert!(
            ProcessLock::acquire(open()).is_err(),
            "an old descriptor must not release the new owner's lock"
        );
        drop(replacement);
        drop(ProcessLock::acquire(open()).unwrap());
        fs::remove_file(path).unwrap();
    }
}
