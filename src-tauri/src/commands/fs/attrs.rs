// ============================================================
// Platform-dependent entry attributes (hidden / read-only /
// executable). The frontend colors entries by these states
// (TICKET-009) and must not have to guess them from the name or
// the Unix mode, so they are determined here once, per platform.
// ============================================================

use std::fs::Metadata;

/// Hidden entry: leading dot on Unix, hidden attribute on Windows.
#[cfg(not(windows))]
pub fn is_hidden(_meta: &Metadata, name: &str) -> bool {
    is_hidden_name(name)
}

#[cfg(windows)]
pub fn is_hidden(meta: &Metadata, name: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 || is_hidden_name(name)
}

/// Hidden state of an entry without metadata (e.g. inside an archive),
/// derived from the name alone.
pub fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.')
}

/// Not writable: no write bit at all on Unix, read-only attribute on Windows.
pub fn is_readonly(meta: &Metadata) -> bool {
    meta.permissions().readonly()
}

/// Executable file: execute bit on Unix, known extension on Windows.
/// Directories are never reported as executable — their `x` bit only
/// grants traversal.
#[cfg(unix)]
pub fn is_executable(meta: &Metadata, _name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    !meta.is_dir() && meta.permissions().mode() & 0o111 != 0
}

#[cfg(windows)]
pub fn is_executable(meta: &Metadata, name: &str) -> bool {
    /// Extensions Windows runs directly.
    const EXECUTABLE_EXTS: &[&str] = &["exe", "bat", "cmd", "com", "msi", "ps1"];
    if meta.is_dir() {
        return false;
    }
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXECUTABLE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

#[cfg(not(any(unix, windows)))]
pub fn is_executable(_meta: &Metadata, _name: &str) -> bool {
    false
}
