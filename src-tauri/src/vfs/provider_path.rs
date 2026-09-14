//! Checks at the legacy streaming adapter boundary, before any provider I/O.
use super::{location::ProviderPath, Scheme};
use std::{io, path::Path};

pub fn text(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if std::path::MAIN_SEPARATOR == '\\' {
        raw.replace('\\', "/")
    } else {
        raw.into_owned()
    }
}

pub fn validate(path: &Path, scheme: Scheme) -> io::Result<()> {
    path.to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "non-UTF8 provider path"))?;
    let raw = text(path);
    if raw.contains("//") || (scheme == Scheme::Smb && raw.contains('\\')) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ambiguous provider path",
        ));
    }
    ProviderPath::from_raw(&raw)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid provider path"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_provider_names_are_not_uri_decoded_again() {
        for raw in ["/a%20b", "/a?b#c", "/é", "/e\u{301}"] {
            validate(Path::new(raw), Scheme::Sftp).unwrap();
            assert_eq!(text(Path::new(raw)), raw);
        }
        assert!(validate(Path::new("/safe//other"), Scheme::Webdav).is_err());
        assert!(validate(Path::new("/safe/../other"), Scheme::Sftp).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn non_utf8_names_fail_instead_of_substituting_another_name() {
        use std::os::unix::ffi::OsStrExt;
        let path = Path::new(std::ffi::OsStr::from_bytes(b"/a\xff"));
        assert!(validate(path, Scheme::Sftp).is_err());
    }
}
