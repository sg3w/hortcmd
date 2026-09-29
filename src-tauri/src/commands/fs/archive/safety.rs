//! Format-independent archive metadata validation. This runs before extraction
//! and also accounts for actual bytes, independently of declared sizes.
use crate::errors::{code, AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Limits {
    pub entries: u64,
    pub total_bytes: u64,
    pub entry_bytes: u64,
    pub depth: usize,
    pub path_bytes: usize,
    pub ratio: u64,
    /// The ceilings packing is measured against (TICKET-395). Separate from
    /// the three above, which defend extraction against an archive somebody
    /// else wrote and are far too small for a directory tree this user picked.
    pub pack_entries: u64,
    pub pack_total_bytes: u64,
    pub pack_entry_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: 100_000,
            total_bytes: 20 * 1024 * 1024 * 1024,
            entry_bytes: 4 * 1024 * 1024 * 1024,
            depth: 64,
            path_bytes: 4096,
            ratio: 1000,
            // A memory bound, not a defence: collection holds one entry and
            // one manifest node per path component before a byte is written,
            // so a million entries is roughly half a gigabyte of bookkeeping.
            // How much fits on the disk is `archive_capacity`'s question.
            pack_entries: 1_000_000,
            pack_total_bytes: 4 * 1024 * 1024 * 1024 * 1024,
            pack_entry_bytes: 1024 * 1024 * 1024 * 1024,
        }
    }
}

impl Limits {
    pub fn load() -> AppResult<Self> {
        let path = crate::config::path("archive-limits.json")?;
        use std::io::Read;
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take(16 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 16 * 1024 {
            return Err(limit());
        }
        let limits: Self = serde_json::from_slice(&bytes)?;
        if limits.entries == 0
            || limits.entry_bytes == 0
            || limits.total_bytes == 0
            || limits.ratio == 0
            || limits.depth == 0
            || limits.path_bytes == 0
            || limits.pack_entries == 0
            || limits.pack_total_bytes == 0
            || limits.pack_entry_bytes == 0
        {
            return Err(limit());
        }
        Ok(limits)
    }

    /// The same rules, measured for writing an archive instead of reading one
    /// (TICKET-395).
    ///
    /// Only the three ceilings move. The per-name rules — depth, length,
    /// reserved and ambiguous spellings — are about what a ZIP member may be
    /// called, which does not depend on the direction, and the ratio guards a
    /// decoder the packer never runs.
    pub fn for_packing(self) -> Self {
        Self {
            entries: self.pack_entries,
            total_bytes: self.pack_total_bytes,
            entry_bytes: self.pack_entry_bytes,
            ..self
        }
    }
}

pub fn unsafe_entry() -> AppError {
    AppError::new(code::ARCHIVE_UNSAFE_ENTRY).op("extract")
}
pub fn limit() -> AppError {
    AppError::new(code::ARCHIVE_LIMIT).op("extract")
}
/// A count refusal, carrying the bound it was measured against (TICKET-395).
/// A limit that names itself is one the user can raise; a bare "a limit was
/// exceeded" is only something to report.
pub fn too_many_entries(limit: u64) -> AppError {
    AppError::new(code::ARCHIVE_TOO_MANY_ENTRIES).with("limit", limit)
}
/// A size refusal, carrying the bound it was measured against (TICKET-395).
pub fn too_large(limit: u64) -> AppError {
    AppError::new(code::ARCHIVE_TOO_LARGE).with("limit", crate::commands::fs::human_size(limit))
}
fn conflict() -> AppError {
    AppError::new(code::ARCHIVE_CONFLICT).op("extract")
}

/// Preserve valid spelling. Normalization is used only for collision detection;
/// silently repairing an unsafe name could overwrite a different member.
pub fn path(name: &str, directory: bool, limits: &Limits) -> AppResult<String> {
    let name = if directory {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    if name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
    {
        return Err(unsafe_entry());
    }
    if name.len() > limits.path_bytes || name.split('/').count() > limits.depth {
        return Err(limit());
    }
    for part in name.split('/') {
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.ends_with(['.', ' '])
            || part.contains([':', '<', '>', '"', '|', '?', '*'])
        {
            return Err(unsafe_entry());
        }
        let stem = part.split('.').next().unwrap_or_default().to_uppercase();
        if matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|n| {
                matches!(
                    n,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        }) {
            return Err(unsafe_entry());
        }
    }
    Ok(name.to_owned())
}

#[derive(Debug)]
struct Node {
    spelling: String,
    directory: bool,
    explicit: bool,
    size: u64,
}

pub struct Manifest {
    pub limits: Limits,
    nodes: HashMap<String, Node>,
    count: u64,
    declared: u64,
    actual: u64,
    compressed: u64,
}

impl Manifest {
    pub fn selected_totals(&self, selected: impl Fn(&str) -> bool) -> AppResult<(u32, u64)> {
        let mut count = 0u32;
        let mut bytes = 0u64;
        for node in self
            .nodes
            .values()
            .filter(|n| !n.directory && selected(&n.spelling))
        {
            count = count
                .checked_add(1)
                .ok_or_else(|| too_many_entries(self.limits.entries))?;
            bytes = bytes
                .checked_add(node.size)
                .ok_or_else(|| too_large(self.limits.total_bytes))?;
        }
        Ok((count, bytes))
    }

    pub fn new(limits: Limits, compressed: u64) -> Self {
        Self {
            limits,
            nodes: HashMap::new(),
            count: 0,
            declared: 0,
            actual: 0,
            compressed,
        }
    }

    pub fn add(&mut self, name: &str, directory: bool, size: u64, link: bool) -> AppResult<String> {
        if link || (directory && size != 0) {
            return Err(unsafe_entry());
        }
        let name = path(name, directory, &self.limits)?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| too_many_entries(self.limits.entries))?;
        if self.count > self.limits.entries {
            return Err(too_many_entries(self.limits.entries));
        }
        if size > self.limits.entry_bytes {
            return Err(too_large(self.limits.entry_bytes));
        }
        self.declared = self
            .declared
            .checked_add(size)
            .ok_or_else(|| too_large(self.limits.total_bytes))?;
        self.check_bytes(self.declared)?;
        let parts: Vec<_> = name.split('/').collect();
        for end in 1..=parts.len() {
            let spelling = parts[..end].join("/");
            let key: String = spelling.nfc().flat_map(char::to_uppercase).nfc().collect();
            let explicit = end == parts.len();
            let is_dir = !explicit || directory;
            match self.nodes.get_mut(&key) {
                Some(node)
                    if node.spelling != spelling
                        || node.directory != is_dir
                        || (explicit && node.explicit) =>
                {
                    return Err(conflict())
                }
                Some(node) => {
                    node.explicit |= explicit;
                    if explicit {
                        node.size = size;
                    }
                }
                None => {
                    self.nodes.insert(
                        key,
                        Node {
                            spelling,
                            directory: is_dir,
                            explicit,
                            size: if explicit { size } else { 0 },
                        },
                    );
                }
            }
        }
        Ok(name)
    }

    pub fn verify(&self, name: &str, directory: bool, size: u64, link: bool) -> AppResult<()> {
        if link {
            return Err(unsafe_entry());
        }
        let spelling = path(name, directory, &self.limits)?;
        let key: String = spelling.nfc().flat_map(char::to_uppercase).nfc().collect();
        match self.nodes.get(&key) {
            Some(node)
                if node.explicit
                    && node.spelling == spelling
                    && node.directory == directory
                    && node.size == size =>
            {
                Ok(())
            }
            _ => Err(AppError::new(code::ARCHIVE_CORRUPT).op("extract-changed")),
        }
    }

    fn check_bytes(&self, bytes: u64) -> AppResult<()> {
        if bytes > self.limits.total_bytes {
            return Err(too_large(self.limits.total_bytes));
        }
        // The ratio is a bomb guard rather than a size the user chose, so it
        // keeps the unnamed code.
        if u128::from(bytes) > u128::from(self.compressed) * u128::from(self.limits.ratio) {
            return Err(limit());
        }
        Ok(())
    }

    /// Charge before writing. A lying size cannot overrun a limit by one buffer.
    pub fn charge(&mut self, file_done: u64, bytes: u64, declared: u64) -> AppResult<u64> {
        let done = file_done
            .checked_add(bytes)
            .ok_or_else(|| too_large(self.limits.entry_bytes))?;
        // A member longer than its own declared size is a lying header, not a
        // ceiling the user could raise; only the second bound is one of ours.
        if done > declared {
            return Err(limit());
        }
        if done > self.limits.entry_bytes {
            return Err(too_large(self.limits.entry_bytes));
        }
        let actual = self
            .actual
            .checked_add(bytes)
            .ok_or_else(|| too_large(self.limits.total_bytes))?;
        self.check_bytes(actual)?;
        self.actual = actual;
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ambiguous_and_platform_dependent_names() {
        for name in [
            "",
            "/x",
            "../x",
            "a/../x",
            "a/./x",
            "a//x",
            "a\\x",
            "C:x",
            "C:/x",
            "//server/x",
            "a\0x",
            "a\nx",
            "x.",
            "x ",
            "CON.txt",
            "sub/LPT²",
            "a:b",
            "NUL",
        ] {
            assert!(path(name, false, &Limits::default()).is_err(), "{name:?}");
        }
        assert_eq!(
            path("日本語/ä.txt", false, &Limits::default()).unwrap(),
            "日本語/ä.txt"
        );
        assert_eq!(path("empty/", true, &Limits::default()).unwrap(), "empty");
    }

    #[test]
    fn detects_conflicts_in_both_orders_and_implicit_parents() {
        for (a, ad, b, bd) in [
            ("a", false, "a", false),
            ("a", true, "a", true),
            ("a", false, "a/b", false),
            ("a/b", false, "a", false),
            ("Dir/a", false, "dir/b", false),
            ("ä", false, "a\u{308}", false),
            ("ß", false, "SS", false),
        ] {
            let mut m = Manifest::new(Limits::default(), 100);
            m.add(a, ad, 0, false).unwrap();
            assert!(m.add(b, bd, 0, false).is_err(), "{a} / {b}");
        }
        let mut m = Manifest::new(Limits::default(), 100);
        m.add("a/b", false, 1, false).unwrap();
        m.add("a/", true, 0, false).unwrap();
        m.add("a/c", false, 1, false).unwrap();
        assert!(m.add("link", false, 0, true).is_err());
    }

    /// A limits file written before TICKET-395 still means what it said, and
    /// the packing ceilings it does not mention are the defaults.
    #[test]
    fn a_limits_file_without_the_pack_ceilings_keeps_its_meaning() {
        let limits: Limits = serde_json::from_str(
            r#"{"entries":5,"totalBytes":6,"entryBytes":7,"depth":8,"pathBytes":9,"ratio":10}"#,
        )
        .unwrap();
        assert_eq!((limits.entries, limits.total_bytes), (5, 6));
        assert_eq!(limits.pack_entries, Limits::default().pack_entries);
        // Packing swaps the three ceilings and keeps every name rule.
        let packing = limits.clone().for_packing();
        assert_eq!(packing.entries, limits.pack_entries);
        assert_eq!(packing.total_bytes, limits.pack_total_bytes);
        assert_eq!(packing.entry_bytes, limits.pack_entry_bytes);
        assert_eq!(
            (packing.depth, packing.path_bytes, packing.ratio),
            (8, 9, 10)
        );
    }

    /// A refusal names the bound it was measured against (TICKET-395), so the
    /// user learns what to raise rather than only that something was hit.
    #[test]
    fn count_and_size_refusals_carry_their_bound() {
        let limits = Limits {
            entries: 1,
            entry_bytes: 2048,
            ..Default::default()
        };
        let mut manifest = Manifest::new(limits, u64::MAX);
        manifest.add("a", false, 0, false).unwrap();
        let error = manifest.add("b", false, 0, false).unwrap_err();
        assert!(error.is(code::ARCHIVE_TOO_MANY_ENTRIES));
        assert_eq!(error.params().get("limit").map(String::as_str), Some("1"));
        // A fresh manifest: the count refusal above would otherwise win again.
        let mut manifest = Manifest::new(
            Limits {
                entry_bytes: 2048,
                ..Default::default()
            },
            u64::MAX,
        );
        let error = manifest.add("c", false, 4096, false).unwrap_err();
        assert!(error.is(code::ARCHIVE_TOO_LARGE));
        assert_eq!(
            error.params().get("limit").map(String::as_str),
            Some("2.0 KB")
        );
    }

    #[test]
    fn checks_metadata_and_real_bytes_with_overflow_safe_arithmetic() {
        let limits = Limits {
            entries: 2,
            total_bytes: 10,
            entry_bytes: 8,
            depth: 2,
            path_bytes: 10,
            ratio: 2,
            ..Default::default()
        };
        let mut m = Manifest::new(limits.clone(), 5);
        m.add("a", false, 8, false).unwrap();
        assert_eq!(m.charge(0, 8, 8).unwrap(), 8);
        assert!(m.charge(8, 1, 8).is_err());
        assert!(m.charge(u64::MAX, 1, u64::MAX).is_err());
        assert!(m.add("b", false, 3, false).is_err());
        let mut m = Manifest::new(limits.clone(), 1);
        assert!(m.add("bomb", false, 3, false).is_err());
        assert!(path("a/b/c", false, &limits).is_err());
        assert!(path("12345678901", false, &limits).is_err());
        let mut m = Manifest::new(limits, u64::MAX);
        m.add("a", true, 0, false).unwrap();
        m.add("b", true, 0, false).unwrap();
        assert!(m.add("c", true, 0, false).is_err());
    }
}
