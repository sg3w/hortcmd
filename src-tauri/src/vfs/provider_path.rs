//! Component validation and traversal at the remote provider boundary.
use super::StreamPath;
use super::{location::ProviderPath, Scheme};
use std::io;

pub fn text(path: &StreamPath) -> String {
    path.to_string_lossy().into_owned()
}

pub fn validate(path: &StreamPath, scheme: Scheme) -> io::Result<()> {
    if !matches!(path, StreamPath::Remote(_)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native path at remote boundary",
        ));
    }
    let raw = text(path);
    if scheme == Scheme::Smb && (raw.contains('\\') || raw.contains("//")) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ambiguous provider path",
        ));
    }
    ProviderPath::from_raw(&raw)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid provider path"))?;
    Ok(())
}

/// Proper remote parents, including an empty directory component. Construct one
/// at a time: deeply nested paths do not allocate all prefixes simultaneously.
pub fn prefixes(path: &StreamPath) -> io::Result<impl Iterator<Item = String> + '_> {
    let StreamPath::Remote(path) = path else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native remote prefix",
        ));
    };
    let mut parts = path.components.as_slice();
    if parts.last().is_some_and(String::is_empty) {
        parts = &parts[..parts.len() - 1];
    }
    Ok((1..=parts.len()).map(move |end| {
        let mut raw = format!("/{}", parts[..end].join("/"));
        if parts[end - 1].is_empty() {
            raw.push('/');
        }
        raw
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_components_and_prefixes_are_preserved() {
        for raw in ["/a%20b", "/a?b#c", "/é", "/e\u{301}", "/safe//other"] {
            let path = StreamPath::remote(raw).unwrap();
            validate(&path, Scheme::Webdav).unwrap();
            assert_eq!(text(&path), raw);
        }
        assert_eq!(
            prefixes(&StreamPath::remote("/a///b").unwrap())
                .unwrap()
                .collect::<Vec<_>>(),
            ["/a", "/a//", "/a///", "/a///b"]
        );
        assert!(StreamPath::remote("/safe/../other").is_err());
        assert!(validate(&StreamPath::from("/a"), Scheme::Webdav).is_err());
        assert!(validate(&StreamPath::remote("/a//b").unwrap(), Scheme::Smb).is_err());
    }
}
