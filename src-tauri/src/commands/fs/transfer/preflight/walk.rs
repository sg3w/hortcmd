//! Depth-first summaries: retain only ancestors and the current sibling-name set.
use crate::vfs::{limits::ProviderLimits, Provider};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use unicode_normalization::UnicodeNormalization;
const MAX_NODES: usize = 1_000_000;
const MAX_NAMES: usize = 50_000;
const MAX_NAME_MEMORY: usize = 16 * 1024 * 1024;
pub fn failure(reason: &'static str) -> io::Error {
    io::Error::other(reason)
}
pub fn folded(name: &str) -> String {
    name.nfc().flat_map(char::to_lowercase).collect()
}
pub fn check_name(path: &Path, limits: &ProviderLimits) -> io::Result<()> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| failure("invalidName"))?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('\0')
        || limits
            .maximum_name_bytes
            .is_some_and(|n| name.len() > n as usize)
        || limits
            .maximum_path_bytes
            .is_some_and(|n| path.as_os_str().len() > n as usize)
        || limits
            .reserved_names
            .as_ref()
            .is_some_and(|names| names.iter().any(|r| folded(r) == folded(name)))
        || limits
            .forbidden_characters
            .as_ref()
            .is_some_and(|chars| name.chars().any(|c| chars.contains(c)))
    {
        return Err(failure("invalidName"));
    }
    Ok(())
}
pub struct Summary {
    pub files: u32,
    pub bytes: u64,
    pub version: String,
}
pub struct Walker<'a> {
    cancel: &'a AtomicBool,
    nodes: usize,
    names: usize,
    name_memory: usize,
    ancestors: HashSet<String>,
}
impl<'a> Walker<'a> {
    pub fn new(cancel: &'a AtomicBool) -> Self {
        Self {
            cancel,
            nodes: 0,
            names: 0,
            name_memory: 0,
            ancestors: HashSet::new(),
        }
    }
    pub fn measure(
        &mut self,
        provider: &dyn Provider,
        source: &Path,
        target: &Path,
        limits: &ProviderLimits,
    ) -> io::Result<Summary> {
        let mut summary = Summary {
            files: 0,
            bytes: 0,
            version: String::new(),
        };
        let mut digest = Sha256::new();
        self.visit(
            provider,
            source,
            target,
            limits,
            0,
            &mut summary,
            &mut digest,
        )?;
        summary.version = hex::encode(digest.finalize());
        Ok(summary)
    }
    #[allow(clippy::too_many_arguments)]
    fn visit(
        &mut self,
        provider: &dyn Provider,
        source: &Path,
        target: &Path,
        limits: &ProviderLimits,
        depth: usize,
        total: &mut Summary,
        digest: &mut Sha256,
    ) -> io::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "preflight cancelled",
            ));
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES || depth > 128 {
            return Err(failure("planningLimit"));
        }
        check_name(target, limits)?;
        let stat = provider.stat(source)?;
        let identity = provider.planning_identity(source)?;
        let before = identity.clone();
        let name = source
            .file_name()
            .and_then(|p| p.to_str())
            .ok_or_else(|| failure("invalidName"))?;
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update(stat.len.to_le_bytes());
        digest.update([u8::from(stat.is_dir)]);
        if let Some(identity) = &identity {
            digest.update(identity.as_bytes());
        }
        if stat.is_dir {
            let key = identity.unwrap_or_else(|| source.to_string_lossy().into_owned());
            if !self.ancestors.insert(key.clone()) {
                return Err(failure("recursiveSource"));
            }
            let mut names = HashSet::new();
            let mut child_summaries = Vec::new();
            provider.visit_dir(source, MAX_NAMES, &mut |child| {
                if child.parent() != Some(source) {
                    return Err(failure("invalidName"));
                }
                let name = child
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| failure("invalidName"))?;
                let key = if limits.case_sensitive == Some(true) {
                    name.to_owned()
                } else {
                    folded(name)
                };
                if !names.insert(key.clone()) {
                    return Err(failure("caseCollision"));
                }
                self.names += 1;
                self.name_memory += key.len();
                if self.names > MAX_NAMES || self.name_memory > MAX_NAME_MEMORY {
                    return Err(failure("planningLimit"));
                }
                // Summaries are fixed-size digests, sorted so provider listing order
                // cannot make an unchanged queued plan appear changed.
                let mut child_hash = Sha256::new();
                self.visit(
                    provider,
                    &child,
                    &target.join(name),
                    limits,
                    depth + 1,
                    total,
                    &mut child_hash,
                )?;
                child_summaries.push((key, child_hash.finalize()));
                Ok(())
            })?;
            child_summaries.sort_by(|a, b| a.0.cmp(&b.0));
            for (_, hash) in child_summaries {
                digest.update(hash);
            }
            self.names -= names.len();
            self.name_memory -= names.iter().map(String::len).sum::<usize>();
            self.ancestors.remove(&key);
        } else {
            if limits
                .maximum_object_bytes
                .is_some_and(|max| stat.len > max)
            {
                return Err(failure("objectLimit"));
            }
            total.files = total
                .files
                .checked_add(1)
                .ok_or_else(|| failure("overflow"))?;
            total.bytes = total
                .bytes
                .checked_add(stat.len)
                .ok_or_else(|| failure("overflow"))?;
        }
        if before != provider.planning_identity(source)? {
            return Err(failure("sourceChanged"));
        }
        Ok(())
    }
}
