//! Paths used by provider I/O. Remote components never acquire host-OS semantics.
use super::location::ProviderPath;
use std::{
    borrow::Cow,
    ffi::OsStr,
    fmt, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StreamPath {
    Local(PathBuf),
    Remote(ProviderPath),
}
impl StreamPath {
    pub fn remote(raw: &str) -> crate::errors::AppResult<Self> {
        Ok(Self::Remote(ProviderPath::from_raw(raw)?))
    }
    pub fn native(&self) -> io::Result<&Path> {
        match self {
            Self::Local(p) => Ok(p),
            Self::Remote(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "remote path at native boundary",
            )),
        }
    }
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        match self {
            Self::Local(p) => p.to_string_lossy(),
            Self::Remote(p) => Cow::Owned(p.raw()),
        }
    }
    pub fn to_path_buf(&self) -> Self {
        self.clone()
    }
    pub fn parent(&self) -> Option<Self> {
        match self {
            Self::Local(p) => p.parent().map(|p| Self::Local(p.into())),
            Self::Remote(p) => (p.raw() != "/").then(|| Self::Remote(p.parent())),
        }
    }
    pub fn file_name(&self) -> Option<&OsStr> {
        match self {
            Self::Local(p) => p.file_name(),
            Self::Remote(p) => {
                let parts = &p.components;
                let index =
                    parts
                        .len()
                        .checked_sub(if parts.last().is_some_and(String::is_empty) {
                            2
                        } else {
                            1
                        })?;
                Some(OsStr::new(&parts[index]))
            }
        }
    }
    pub fn join(&self, name: impl AsRef<OsStr>) -> io::Result<Self> {
        match self {
            Self::Local(p) => Ok(Self::Local(p.join(name.as_ref()))),
            Self::Remote(p) => {
                let name = name.as_ref().to_str().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "non-UTF8 provider component")
                })?;
                Ok(Self::Remote(p.join(name).map_err(io::Error::other)?))
            }
        }
    }
    pub fn file_stem(&self) -> Option<&OsStr> {
        match self {
            Self::Local(p) => p.file_stem(),
            Self::Remote(_) => {
                let name = self.file_name()?.to_str()?;
                Some(OsStr::new(match name.rsplit_once('.') {
                    Some((stem, _)) if !stem.is_empty() => stem,
                    _ => name,
                }))
            }
        }
    }
    pub fn extension(&self) -> Option<&OsStr> {
        match self {
            Self::Local(p) => p.extension(),
            Self::Remote(_) => {
                let (stem, ext) = self.file_name()?.to_str()?.rsplit_once('.')?;
                (!stem.is_empty()).then(|| OsStr::new(ext))
            }
        }
    }
    pub fn starts_with(&self, root: &Self) -> bool {
        match (self, root) {
            (Self::Local(p), Self::Local(r)) => p.starts_with(r),
            (Self::Remote(p), Self::Remote(r)) => r.contains(p),
            _ => false,
        }
    }
    pub fn display(&self) -> impl fmt::Display + '_ {
        self.to_string_lossy()
    }
}
impl From<PathBuf> for StreamPath {
    fn from(p: PathBuf) -> Self {
        Self::Local(p)
    }
}
impl From<&Path> for StreamPath {
    fn from(p: &Path) -> Self {
        Self::Local(p.into())
    }
}
impl From<&str> for StreamPath {
    fn from(p: &str) -> Self {
        Self::Local(p.into())
    }
}
impl From<String> for StreamPath {
    fn from(p: String) -> Self {
        Self::Local(p.into())
    }
}
impl From<&String> for StreamPath {
    fn from(p: &String) -> Self {
        Self::Local(p.into())
    }
}
impl From<&StreamPath> for StreamPath {
    fn from(p: &StreamPath) -> Self {
        p.clone()
    }
}

impl Default for StreamPath {
    fn default() -> Self {
        Self::Local(PathBuf::new())
    }
}

impl From<&PathBuf> for StreamPath {
    fn from(p: &PathBuf) -> Self {
        Self::Local(p.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_parent_join_and_basename_keep_empty_components() {
        for raw in [
            "/a/b",
            "/a//b",
            "/a///b",
            "//b",
            "/a//é.txt",
            "/a//a\\b.txt",
        ] {
            let path = StreamPath::remote(raw).unwrap();
            let parent = path.parent().unwrap();
            assert_eq!(parent.join(path.file_name().unwrap()).unwrap(), path);
            assert_eq!(path.to_string_lossy(), raw);
            assert!(path.native().is_err());
        }
        assert_eq!(
            StreamPath::remote("/a//b")
                .unwrap()
                .parent()
                .unwrap()
                .to_string_lossy(),
            "/a//"
        );
        assert_ne!(
            StreamPath::remote("/a/b").unwrap(),
            StreamPath::remote("/a//b").unwrap()
        );
        let root = StreamPath::remote("/a//").unwrap();
        assert!(StreamPath::remote("/a//b").unwrap().starts_with(&root));
        assert!(!StreamPath::remote("/a/b").unwrap().starts_with(&root));
    }
    #[cfg(unix)]
    #[test]
    fn non_utf8_local_names_never_become_replacement_characters_remotely() {
        use std::os::unix::ffi::OsStrExt;
        let name = OsStr::from_bytes(b"a\xff");
        assert!(StreamPath::remote("/dest").unwrap().join(name).is_err());
        assert_eq!(
            StreamPath::from("/dest")
                .join(name)
                .unwrap()
                .native()
                .unwrap()
                .file_name(),
            Some(name)
        );
    }
}
