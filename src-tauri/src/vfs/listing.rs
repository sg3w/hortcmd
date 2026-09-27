// ============================================================
// One shared bound on how large a directory listing may grow
// (ticket 321).
//
// How many entries a directory has is the *server's* answer, so a
// malicious or compromised peer can answer an ordinary "open this
// folder" with millions of fabricated names. Collecting them first and
// checking the count afterwards bounds nothing — the allocation has
// already happened. Every provider therefore charges each entry to a
// `ListingBudget` while it walks the protocol response and stops at the
// first entry past the cap.
// ============================================================

use crate::errors::{code, AppError, AppResult};

/// Entries one directory listing may report before the client refuses it.
/// Far above any directory a person browses, far below what would hurt.
pub const MAX_ENTRIES: usize = 200_000;

/// Bytes of entry names one listing may accumulate. A listing can stay
/// under `MAX_ENTRIES` and still be huge if every name is long.
pub const MAX_NAME_BYTES: usize = 16 * 1024 * 1024;

/// Counts a listing while it is read and refuses it once it is too large.
pub struct ListingBudget {
    max_entries: usize,
    entries: usize,
    name_bytes: usize,
}

impl Default for ListingBudget {
    fn default() -> Self {
        Self::new(MAX_ENTRIES)
    }
}

impl ListingBudget {
    /// A budget capped at `max_entries`, never above the global maximum.
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries: max_entries.min(MAX_ENTRIES),
            entries: 0,
            name_bytes: 0,
        }
    }

    /// Charges one entry. The error is the provider's signal to abandon the
    /// enumeration instead of reading the rest of the server's answer.
    pub fn charge(&mut self, name: &str) -> AppResult<()> {
        self.entries += 1;
        self.name_bytes += name.len();
        if self.entries > self.max_entries {
            return Err(AppError::new(code::FS_TOO_MANY_ENTRIES).with("limit", self.max_entries));
        }
        if self.name_bytes > MAX_NAME_BYTES {
            return Err(AppError::new(code::FS_TOO_MANY_ENTRIES).with("limit", self.max_entries));
        }
        Ok(())
    }

    /// Same, for the `io::Result` paths of the `Provider` trait.
    pub fn charge_io(&mut self, name: &str) -> std::io::Result<()> {
        self.charge(name)
            .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_is_refused_at_the_entry_cap_not_after_it() {
        let mut budget = ListingBudget::new(2);
        assert!(budget.charge("a").is_ok());
        assert!(budget.charge("b").is_ok());
        let err = budget.charge("c").expect_err("third entry is over the cap");
        assert_eq!(&*err.code, code::FS_TOO_MANY_ENTRIES);
    }

    #[test]
    fn the_global_cap_wins_over_a_larger_caller_limit() {
        let budget = ListingBudget::new(usize::MAX);
        assert_eq!(budget.max_entries, MAX_ENTRIES);
    }

    #[test]
    fn long_names_are_bounded_even_below_the_entry_cap() {
        let mut budget = ListingBudget::default();
        let name = "n".repeat(64 * 1024);
        let mut charged = 0usize;
        while budget.charge(&name).is_ok() {
            charged += 1;
            assert!(
                charged < MAX_ENTRIES,
                "name bytes never bounded the listing"
            );
        }
        assert!(charged * name.len() >= MAX_NAME_BYTES - name.len());
    }
}
