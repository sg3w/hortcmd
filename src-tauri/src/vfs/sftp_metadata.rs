//! SFTP mode types are discriminants, not independently combinable flags.
use crate::commands::fs::dir::DirEntry;
use russh_sftp::protocol::FileAttributes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Directory,
    File,
    Symlink,
    Special,
}

pub(crate) fn kind(attrs: &FileAttributes) -> Option<EntryKind> {
    match attrs.permissions? & 0xf000 {
        0x4000 => Some(EntryKind::Directory),
        0x8000 => Some(EntryKind::File),
        0xa000 => Some(EntryKind::Symlink),
        0x1000 | 0x2000 | 0x5000 | 0x6000 | 0xc000 => Some(EntryKind::Special),
        _ => None,
    }
}
pub(super) fn stat(attrs: &FileAttributes) -> super::Stat {
    super::Stat {
        is_dir: kind(attrs) == Some(EntryKind::Directory),
        len: attrs.size.unwrap_or(0),
    }
}
pub(super) fn listing_entry(
    name: String,
    attrs: &FileAttributes,
    target: Option<&FileAttributes>,
) -> DirEntry {
    let entry_kind = kind(attrs);
    let is_symlink = entry_kind == Some(EntryKind::Symlink);
    let target = if is_symlink { target } else { None };
    let is_dir = match target {
        Some(target) => kind(target) == Some(EntryKind::Directory),
        None => entry_kind == Some(EntryKind::Directory),
    };
    let size = match target {
        _ if is_dir => 0,
        Some(target) => target.size.unwrap_or(0),
        None if is_symlink => 0,
        None => attrs.size.unwrap_or(0),
    };
    let mode = attrs.permissions.unwrap_or(0);
    DirEntry {
        hidden: name.starts_with('.'),
        readonly: mode != 0 && mode & 0o200 == 0,
        executable: !is_dir && mode & 0o111 != 0,
        name,
        is_dir,
        is_symlink,
        size,
        modified: attrs.mtime.unwrap_or(0) as u64,
        // Preserve link-level permissions for presentation, even when resolving a target.
        mode: attrs.permissions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn attrs(mode: Option<u32>) -> FileAttributes {
        FileAttributes {
            permissions: mode,
            size: Some(17),
            mtime: Some(123),
            ..Default::default()
        }
    }
    #[test]
    fn exact_types_drive_stat_listing_and_recursive_directory_decisions() {
        for (mode, expected) in [
            (0x4000, Some(EntryKind::Directory)),
            (0x8000, Some(EntryKind::File)),
            (0xa000, Some(EntryKind::Symlink)),
            (0x1000, Some(EntryKind::Special)),
            (0x2000, Some(EntryKind::Special)),
            (0x5000, Some(EntryKind::Special)),
            (0x6000, Some(EntryKind::Special)),
            (0xc000, Some(EntryKind::Special)),
            (0, None),
            (0xe000, None),
            (0xf000, None),
        ] {
            for permissions in [0, 0o644, 0o755, 0o7777] {
                let attrs = attrs(Some(mode | permissions));
                assert_eq!(kind(&attrs), expected, "mode {mode:x}");
                let directory = expected == Some(EntryKind::Directory);
                assert_eq!(stat(&attrs).is_dir, directory);
                let entry = listing_entry("entry".into(), &attrs, None);
                assert_eq!(entry.is_dir, directory);
                assert_eq!(entry.is_symlink, expected == Some(EntryKind::Symlink));
                assert_eq!(entry.mode, attrs.permissions);
                assert_eq!(entry.modified, 123);
            }
        }
        let missing = attrs(None);
        assert_eq!(kind(&missing), None);
        assert!(!stat(&missing).is_dir);
        let entry = listing_entry("unknown".into(), &missing, None);
        assert!(!entry.is_dir && !entry.is_symlink);
    }
    #[test]
    fn resolved_and_broken_links_preserve_link_presentation_without_false_directories() {
        let link = attrs(Some(0xa000 | 0o755));
        for mode in [0x4000, 0x8000, 0x6000, 0xc000, 0xe000] {
            let target = attrs(Some(mode));
            let entry = listing_entry("link".into(), &link, Some(&target));
            assert!(entry.is_symlink);
            assert_eq!(entry.is_dir, mode == 0x4000);
            assert_eq!(entry.size, if mode == 0x4000 { 0 } else { 17 });
            assert_eq!(entry.mode, link.permissions);
        }
        let broken = listing_entry("broken".into(), &link, None);
        assert!(broken.is_symlink && !broken.is_dir);
        assert_eq!(broken.size, 0);
    }
    #[test]
    fn regular_entry_attributes_keep_existing_size_permissions_and_visibility() {
        let entry = listing_entry(".script".into(), &attrs(Some(0x8000 | 0o555)), None);
        assert!(entry.hidden && entry.readonly && entry.executable);
        assert_eq!(entry.size, 17);
        assert!(!entry.is_dir && !entry.is_symlink);
    }
}
