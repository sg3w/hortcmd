//! Provider adapter for recursive MCP transfers. Every descendant and every
//! publication is checked through the same scope policy as a top-level call.
use super::{execution, scope, McpConfig, McpTier};
use crate::errors::AppResult;
use crate::vfs::{self, Provider, Stat};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};

pub struct ScopedProvider {
    inner: Arc<dyn Provider>,
    prefix: String,
    config: McpConfig,
    stage: Option<PathBuf>,
    stage_created: AtomicBool,
}

impl ScopedProvider {
    pub fn resolve(raw: &str, config: &McpConfig) -> AppResult<(Self, PathBuf)> {
        let (inner, path) = vfs::resolve(raw)?;
        let address = vfs::parse_address(raw)?;
        let prefix = if address.scheme() == vfs::Scheme::Local {
            String::new()
        } else {
            let scheme_end = raw.find("://").unwrap_or(0) + 3;
            let end = raw[scheme_end..]
                .find('/')
                .map_or(raw.len(), |i| scheme_end + i);
            raw[..end].to_owned()
        };
        let path = PathBuf::from(path);
        let path = if prefix.is_empty() {
            match (path.parent(), path.file_name()) {
                (Some(parent), Some(name)) => std::fs::canonicalize(parent)?.join(name),
                _ => std::fs::canonicalize(path)?,
            }
        } else {
            path
        };
        let provider = Self {
            inner,
            prefix,
            config: config.clone(),
            stage: None,
            stage_created: AtomicBool::new(false),
        };
        provider.check(&path)?;
        Ok((provider, path))
    }

    /// Bind this destination adapter to one freshly planned staging path.
    /// A filename prefix alone never grants cleanup permission.
    pub fn track_stage(&mut self, path: &Path) -> io::Result<()> {
        self.check(path)?;
        if self.stage.is_some() {
            return Err(io::Error::other("staging already registered"));
        }
        match self.inner.stat(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "staging exists",
                ))
            }
        }
        self.stage = Some(path.to_owned());
        Ok(())
    }

    fn created(&self, path: &Path) {
        if self.stage.as_deref() == Some(path) {
            self.stage_created.store(true, Ordering::Relaxed);
        }
    }

    /// Called after the transfer has released its readers and writers. Ordinary
    /// remove/rename operations retain cancellation checks, including source deletion.
    pub fn cleanup_stage(&self, path: &Path) -> io::Result<()> {
        if self.stage.as_deref() != Some(path) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unowned staging path",
            ));
        }
        self.check_scope(path)?;
        let stat = match self.inner.stat(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            result => result?,
        };
        if !self.stage_created.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "staging creation not confirmed",
            ));
        }
        if stat.is_dir {
            self.inner.remove_dir_all(path)?;
        } else {
            self.inner.remove_file(path)?;
        }
        self.stage_created.store(false, Ordering::Relaxed);
        Ok(())
    }

    pub fn address(&self, path: &Path) -> io::Result<String> {
        let text = path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "non-UTF8 MCP path"))?;
        if self.prefix.is_empty() {
            Ok(text.to_owned())
        } else {
            Ok(format!(
                "{}{}",
                self.prefix,
                vfs::location::ProviderPath::from_raw(text)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?
                    .uri()
            ))
        }
    }

    pub fn check(&self, path: &Path) -> io::Result<()> {
        if execution::cancel_flag().load(std::sync::atomic::Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "MCP request cancelled",
            ));
        }
        self.check_scope(path)
    }

    // Cleanup may ignore cancellation, but never a live scope/permission change.
    fn check_scope(&self, path: &Path) -> io::Result<()> {
        let current = execution::live_config();
        let config = current.as_ref().unwrap_or(&self.config);
        if (current.is_some() && !config.enabled) || !config.allows(McpTier::Write) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "MCP write permission revoked",
            ));
        }
        scope::require_in_scope(&config.roots, &self.address(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e.to_string()))?;
        // Recursive copies must not follow even an in-scope link: a link back
        // to a parent would recurse forever, and a moved link is not its target.
        if self.prefix.is_empty() {
            for ancestor in path.ancestors() {
                match std::fs::symlink_metadata(ancestor) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "symbolic links cannot be transferred through MCP",
                        ))
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(())
    }
}

impl Provider for ScopedProvider {
    fn store_id(&self) -> String {
        self.inner.store_id()
    }
    fn list(&self, path: &str) -> AppResult<crate::commands::fs::dir::DirListing> {
        self.check(Path::new(path))?;
        self.inner.list(path)
    }
    fn stat(&self, path: &Path) -> io::Result<Stat> {
        self.check(path)?;
        self.inner.stat(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.check(path).is_ok() && self.inner.exists(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.stat(path).is_ok_and(|s| s.is_dir)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.check(path)?;
        let children = self.inner.read_dir(path)?;
        for child in &children {
            if child.parent() != Some(path) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "provider returned a non-child path",
                ));
            }
            self.check(child)?;
        }
        Ok(children)
    }
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        self.check(path)?;
        self.inner.open_read(path)
    }
    fn create_write(&self, path: &Path) -> io::Result<Box<dyn Write + Send>> {
        self.check(path)?;
        let writer = self.inner.create_write(path)?;
        self.created(path);
        Ok(writer)
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.check(path)?;
        self.inner.write(path, data)
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.check(path)?;
        self.inner.create_dir_all(path)?;
        self.created(path);
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.check(from)?;
        self.check(to)?;
        self.inner.rename(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.check(path)?;
        self.inner.remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.check(path)?;
        self.inner.remove_dir_all(path)
    }
    fn supports_staging(&self) -> bool {
        self.inner.supports_staging()
    }
    fn rename_replaces(&self) -> bool {
        self.inner.rename_replaces()
    }
    fn transport_security(&self) -> Option<crate::net::security::TransportInfo> {
        self.inner.transport_security()
    }
    fn transport_alive(&self) -> bool {
        self.inner.transport_alive()
    }
}
