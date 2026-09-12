// ============================================================
// Location capabilities (ticket 031).
//
// The VFS `Provider` trait covers browsing, preview, transfers,
// create, rename and delete — that is what every provider really
// implements. Everything else (trash, Quick Look, Git, Finder tags,
// search, archives, comparisons, volume data, recursive size,
// extended properties, local terminals) still runs against
// `std::fs` or a platform API and therefore only works on a *local*
// address.
//
// This module is the single place that says so. It answers, per
// address, which operations that location supports right now, and the
// commands themselves refuse an address that lacks the capability they
// need — so a remote address is never handed to a local-only API where
// it would be read as a relative path.
//
// Two rules keep the matrix honest:
//   * a capability is advertised only where its command is actually
//     routed through the VFS and covered by tests — never because a
//     provider merely has `open_read`;
//   * remote capabilities are reported for a *live* session only. A
//     disconnected location supports nothing, which is exactly what the
//     UI must show after a connection drops.
// ============================================================

use super::{parse_address, Address, Scheme};
use crate::errors::{code, AppError, AppResult, ErrorAction};
use serde::Serialize;
use ts_rs::TS;

/// The code a refusal carries. Since ticket 036 this is a real error
/// code rather than a marker embedded in a sentence, so the frontend
/// branches on it and renders the wording itself.
#[allow(unused_imports)] // the name other modules and tests use
pub use crate::errors::code::CAPABILITY_UNSUPPORTED as UNSUPPORTED;

/// A single operation a location may or may not support. The
/// identifiers are stable, UI-independent, and deliberately finer than
/// the menu labels: copy source and copy destination, archive source
/// and archive destination, are separate questions for a dialog that
/// combines two locations.
///
/// New identifiers are appended; existing ones keep their name, because
/// the planned plugin and MCP surfaces (tickets 201–208, 041) will hand
/// these very strings out.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Hash, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum Capability {
    /// List a directory.
    Browse,
    /// Read file contents for the viewer (F3).
    Preview,
    /// Act as the source of a copy.
    CopyFrom,
    /// Act as the destination of a copy.
    CopyTo,
    /// Rename inside the location / act as the source of a move.
    Move,
    /// Create directories and write whole files.
    Create,
    /// Delete permanently (no restore).
    DeletePermanent,
    /// Delete into the OS trash (restorable).
    Trash,
    /// Browse an archive that lies at this location.
    ArchiveBrowse,
    /// Hold the archive an extraction reads from.
    ArchiveExtractFrom,
    /// Receive the files an extraction writes.
    ArchiveExtractTo,
    /// Hold the files a pack operation reads.
    ArchivePackFrom,
    /// Receive the archive a pack operation writes.
    ArchivePackTo,
    /// Report the detailed properties of an entry.
    Stat,
    /// Add up the size of a directory tree.
    RecursiveSize,
    /// Compute file checksums.
    Checksums,
    /// Read and write access rights / ownership.
    Permissions,
    /// Search a tree for files.
    Search,
    /// Compare and synchronize two directory trees.
    CompareDirs,
    /// Compare two files line by line.
    CompareFiles,
    /// Report the Git status of a directory.
    GitStatus,
    /// Read and write Finder tags.
    FinderTags,
    /// Report capacity/usage of the volume behind a path.
    VolumeInfo,
    /// Hand a path to the OS ("Reveal in Finder", default program).
    SystemOpen,
    /// Open an entry with a chosen local application.
    OpenWith,
    /// macOS Quick Look preview.
    QuickLook,
    /// Start the configured terminal program in the folder.
    LocalTerminal,
    /// Open a shell on the connection carrying this location.
    SshTerminal,
}

impl Capability {
    /// Every identifier, in declaration order — the enumeration of the
    /// contract. Its consumers today are the tests that keep
    /// [`id`](Capability::id) and the serialized form in step.
    #[cfg(test)]
    pub const ALL: [Capability; 28] = [
        Capability::Browse,
        Capability::Preview,
        Capability::CopyFrom,
        Capability::CopyTo,
        Capability::Move,
        Capability::Create,
        Capability::DeletePermanent,
        Capability::Trash,
        Capability::ArchiveBrowse,
        Capability::ArchiveExtractFrom,
        Capability::ArchiveExtractTo,
        Capability::ArchivePackFrom,
        Capability::ArchivePackTo,
        Capability::Stat,
        Capability::RecursiveSize,
        Capability::Checksums,
        Capability::Permissions,
        Capability::Search,
        Capability::CompareDirs,
        Capability::CompareFiles,
        Capability::GitStatus,
        Capability::FinderTags,
        Capability::VolumeInfo,
        Capability::SystemOpen,
        Capability::OpenWith,
        Capability::QuickLook,
        Capability::LocalTerminal,
        Capability::SshTerminal,
    ];

    /// The serialized identifier — also what an `UNSUPPORTED` error
    /// names, so backend and frontend talk about the same string. A
    /// test asserts this stays identical to the serde output.
    pub fn id(self) -> &'static str {
        match self {
            Capability::Browse => "browse",
            Capability::Preview => "preview",
            Capability::CopyFrom => "copyFrom",
            Capability::CopyTo => "copyTo",
            Capability::Move => "move",
            Capability::Create => "create",
            Capability::DeletePermanent => "deletePermanent",
            Capability::Trash => "trash",
            Capability::ArchiveBrowse => "archiveBrowse",
            Capability::ArchiveExtractFrom => "archiveExtractFrom",
            Capability::ArchiveExtractTo => "archiveExtractTo",
            Capability::ArchivePackFrom => "archivePackFrom",
            Capability::ArchivePackTo => "archivePackTo",
            Capability::Stat => "stat",
            Capability::RecursiveSize => "recursiveSize",
            Capability::Checksums => "checksums",
            Capability::Permissions => "permissions",
            Capability::Search => "search",
            Capability::CompareDirs => "compareDirs",
            Capability::CompareFiles => "compareFiles",
            Capability::GitStatus => "gitStatus",
            Capability::FinderTags => "finderTags",
            Capability::VolumeInfo => "volumeInfo",
            Capability::SystemOpen => "systemOpen",
            Capability::OpenWith => "openWith",
            Capability::QuickLook => "quickLook",
            Capability::LocalTerminal => "localTerminal",
            Capability::SshTerminal => "sshTerminal",
        }
    }
}

/// What a location supports right now, plus the identity to cache it
/// under. The frontend keys its cache on `store_id`, so one answer
/// serves every menu item of that location instead of one IPC call per
/// item.
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct LocationCapabilities {
    /// Canonical scheme of the address (`local`, `sftp`, `ftp`, …).
    pub scheme: String,
    /// Cache identity: `"local"` for the local file system, otherwise
    /// `"sftp:user@host:port"` — the provider's own `store_id`.
    pub store_id: String,
    /// Whether a live session serves this address. Always true for
    /// local; false for a remote address whose connection is gone.
    pub connected: bool,
    /// Every capability this location offers in its current state.
    pub caps: Vec<Capability>,
}

impl LocationCapabilities {
    pub fn has(&self, cap: Capability) -> bool {
        self.caps.contains(&cap)
    }
}

/// The local matrix, gated by platform. Local keeps everything it
/// currently does: the unix-only rights editing, the macOS-only Quick
/// Look and Finder tags, and the OS integrations (trash, system open,
/// terminal) that the `trash`/`open` crates provide on all three
/// desktop platforms.
fn local_caps() -> Vec<Capability> {
    use Capability::*;
    let mut caps = vec![
        Browse,
        Preview,
        CopyFrom,
        CopyTo,
        Move,
        Create,
        DeletePermanent,
        Trash,
        ArchiveBrowse,
        ArchiveExtractFrom,
        ArchiveExtractTo,
        ArchivePackFrom,
        ArchivePackTo,
        Stat,
        RecursiveSize,
        Checksums,
        Search,
        CompareDirs,
        CompareFiles,
        GitStatus,
        VolumeInfo,
        SystemOpen,
        OpenWith,
        LocalTerminal,
    ];
    if cfg!(unix) {
        // chmod/chown exist; Windows rights are a different model and
        // are not edited here.
        caps.push(Permissions);
    }
    if cfg!(target_os = "macos") {
        caps.push(QuickLook);
        caps.push(FinderTags);
    }
    caps
}

/// The core set every connected remote provider really implements
/// through the VFS trait: list, stat-for-preview, the streaming
/// transfer primitives, mkdir, rename and remove.
///
/// Deliberately nothing else. Search, comparisons, archives, checksums,
/// recursive size and extended properties are still local-only command
/// implementations; each becomes a remote capability in the ticket that
/// routes it through the VFS and tests it against a live server.
fn remote_core_caps() -> Vec<Capability> {
    use Capability::*;
    vec![
        Browse,
        Preview,
        CopyFrom,
        CopyTo,
        Move,
        Create,
        DeletePermanent,
    ]
}

/// Capabilities of an already parsed address.
///
/// A remote address is answered from its live session: the registry
/// says whether anything is connected, and the provider itself says
/// whether it carries an SSH connection — so "SFTP offers a shell,
/// FTP/SMB/WebDAV do not" is read off the provider rather than matched
/// against a scheme name.
pub fn for_parsed(addr: &Address) -> AppResult<LocationCapabilities> {
    if addr.scheme == Scheme::Local {
        return Ok(LocationCapabilities {
            scheme: Scheme::Local.as_str().to_string(),
            store_id: "local".to_string(),
            connected: true,
            caps: local_caps(),
        });
    }

    let scheme = addr.scheme.as_str().to_string();
    let key = addr.connection_key().ok_or_else(|| {
        AppError::new(code::VALIDATION_BAD_ADDRESS)
            .with("scheme", scheme.to_uppercase())
            .action(ErrorAction::Report)
    })?;
    let store_id = format!("{}:{}", scheme, key);

    let Ok(provider) = crate::net::session::remote_provider(&scheme, &key) else {
        // Nothing connected: the location supports nothing at all. That
        // is the honest answer and the one the UI needs after a drop.
        return Ok(LocationCapabilities {
            scheme,
            store_id,
            connected: false,
            caps: Vec::new(),
        });
    };

    let mut caps = remote_core_caps();
    if provider.ssh().is_some() {
        caps.push(Capability::SshTerminal);
    }
    Ok(LocationCapabilities {
        scheme,
        store_id,
        connected: true,
        caps,
    })
}

/// Capabilities of a raw address string. Resolves through the same
/// parser and session registry as every file-system command, so the
/// answer describes the location the commands would actually act on.
pub fn for_address(raw: &str) -> AppResult<LocationCapabilities> {
    for_parsed(&parse_address(raw)?)
}

/// Typed capability query for the frontend.
#[tauri::command]
pub fn location_capabilities(address: String) -> AppResult<LocationCapabilities> {
    for_address(&address)
}

/// Gate every local-only command puts in front of its work: refuses an
/// address whose location does not offer `cap`, with an error carrying
/// the [`UNSUPPORTED`] marker and the capability id.
///
/// Backend commands stay authoritative — a frontend that (wrongly)
/// offers a disabled action, or a direct IPC call, is stopped here and
/// never reaches an API that would read `sftp://host/x` as a relative
/// local path.
pub fn require(raw: &str, cap: Capability) -> AppResult<()> {
    let caps = for_address(raw)?;
    if caps.has(cap) {
        return Ok(());
    }
    Err(unsupported(raw, cap, &caps))
}

/// Same gate for both ends of a two-location operation (copy, pack,
/// extract, compare, synchronize): each side is asked for its own
/// capability, so an allowed source never implies an allowed target.
pub fn require_pair(
    from: &str,
    from_cap: Capability,
    to: &str,
    to_cap: Capability,
) -> AppResult<()> {
    require(from, from_cap)?;
    require(to, to_cap)
}

/// The typed refusal. Which of the two codes applies is a real
/// distinction: a location that supports nothing has lost its
/// connection and can be reconnected; one that supports other things
/// simply does not implement this operation.
fn unsupported(raw: &str, cap: Capability, caps: &LocationCapabilities) -> AppError {
    if !caps.connected {
        return AppError::new(code::CAPABILITY_OFFLINE)
            .with("address", raw)
            .with("capability", cap.id())
            .provider(&caps.scheme)
            .action(ErrorAction::Reconnect);
    }
    AppError::new(code::CAPABILITY_UNSUPPORTED)
        .with("capability", cap.id())
        .with("scheme", caps.scheme.to_uppercase())
        .provider(&caps.scheme)
        .action(ErrorAction::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_the_serialized_names() {
        // The wire format and `id()` must not drift apart: the frontend
        // matches the serialized strings, the errors carry `id()`.
        for cap in Capability::ALL {
            let json = serde_json::to_value(cap).unwrap();
            assert_eq!(json.as_str(), Some(cap.id()), "wire name of {:?}", cap);
        }
        assert_eq!(Capability::DeletePermanent.id(), "deletePermanent");
    }

    #[test]
    fn every_identifier_is_unique() {
        let mut ids: Vec<&str> = Capability::ALL.iter().map(|c| c.id()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate capability identifier");
    }

    #[test]
    fn local_keeps_its_features() {
        let caps = for_address("/tmp/x").unwrap();
        assert_eq!(caps.scheme, "local");
        assert_eq!(caps.store_id, "local");
        assert!(caps.connected);
        for cap in [
            Capability::Browse,
            Capability::Trash,
            Capability::Search,
            Capability::GitStatus,
            Capability::VolumeInfo,
            Capability::ArchiveBrowse,
            Capability::RecursiveSize,
            Capability::CompareDirs,
            Capability::SystemOpen,
            Capability::LocalTerminal,
        ] {
            assert!(caps.has(cap), "local should offer {}", cap.id());
        }
        // The local file system has no SSH connection of its own.
        assert!(!caps.has(Capability::SshTerminal));
    }

    #[test]
    fn local_platform_gates_follow_the_build() {
        let caps = for_address("/tmp/x").unwrap();
        assert_eq!(caps.has(Capability::Permissions), cfg!(unix));
        assert_eq!(caps.has(Capability::QuickLook), cfg!(target_os = "macos"));
        assert_eq!(caps.has(Capability::FinderTags), cfg!(target_os = "macos"));
    }

    #[test]
    fn file_url_is_a_local_location() {
        let caps = for_address("file:///Users/x").unwrap();
        assert_eq!(caps.store_id, "local");
        assert!(caps.has(Capability::QuickLook) == cfg!(target_os = "macos"));
    }

    #[test]
    fn disconnected_remote_supports_nothing() {
        // No session is registered in the unit-test process, so every
        // remote scheme is reported as disconnected with an empty set.
        for raw in [
            "sftp://u@host/srv",
            "ftp://u@files.example/pub",
            "smb://u@nas.local/share",
            "webdav://u@dav.example/remote",
        ] {
            let caps = for_address(raw).unwrap();
            assert!(!caps.connected, "{} should be offline", raw);
            assert!(caps.caps.is_empty(), "{} should offer nothing", raw);
            assert!(!caps.has(Capability::Browse));
        }
    }

    #[test]
    fn store_id_matches_the_provider_identity() {
        let caps = for_address("sftp://alice@host.example:2222/srv").unwrap();
        assert_eq!(caps.store_id, "sftp:alice@host.example:2222");
        assert_eq!(caps.scheme, "sftp");
        // Default ports fill in, so two spellings of one server share a key.
        assert_eq!(
            for_address("ftp://bob@files.example/pub").unwrap().store_id,
            "ftp:bob@files.example:21"
        );
    }

    #[test]
    fn remote_core_set_is_exactly_the_vfs_trait() {
        // Guards the "do not infer support from open_read" rule: the
        // remote set must never silently grow.
        let core = remote_core_caps();
        assert_eq!(core.len(), 7);
        for cap in [
            Capability::Search,
            Capability::RecursiveSize,
            Capability::Checksums,
            Capability::CompareDirs,
            Capability::CompareFiles,
            Capability::ArchiveBrowse,
            Capability::Trash,
            Capability::SystemOpen,
            Capability::QuickLook,
            Capability::LocalTerminal,
            Capability::GitStatus,
            Capability::FinderTags,
            Capability::VolumeInfo,
            Capability::Stat,
            Capability::Permissions,
        ] {
            assert!(
                !core.contains(&cap),
                "{} is a local-only command implementation",
                cap.id()
            );
        }
    }

    #[test]
    fn unknown_scheme_is_an_error_not_an_empty_set() {
        assert!(for_address("gopher://host/x").is_err());
        assert!(require("gopher://host/x", Capability::Browse).is_err());
    }

    #[test]
    fn remote_without_host_is_an_error() {
        // `sftp:///path` parses but names no server — refusing beats
        // guessing a connection.
        assert!(for_address("sftp:///srv/data").is_err());
    }

    #[test]
    fn require_passes_a_supported_local_action() {
        assert!(require("/tmp/x", Capability::Browse).is_ok());
        assert!(require("/tmp/x", Capability::Trash).is_ok());
    }

    #[test]
    fn require_refuses_a_remote_local_only_action() {
        let err = require("sftp://u@host/srv", Capability::Trash).unwrap_err();
        // Nothing is connected in the unit-test process, so the refusal
        // is the *offline* one — the distinction matters: it can be
        // reconnected, an unsupported capability cannot.
        assert!(err.is(code::CAPABILITY_OFFLINE), "{:?}", err);
        assert_eq!(
            err.params.get("capability").map(String::as_str),
            Some("trash")
        );
    }

    #[test]
    fn require_pair_checks_both_ends() {
        // Source fine, destination remote → refused.
        let err = require_pair(
            "/tmp/src",
            Capability::ArchivePackFrom,
            "sftp://u@host/dst.zip",
            Capability::ArchivePackTo,
        )
        .unwrap_err();
        assert!(err.is(code::CAPABILITY_OFFLINE), "{:?}", err);
        // Destination fine, source remote → refused as well.
        let err = require_pair(
            "sftp://u@host/src",
            Capability::ArchivePackFrom,
            "/tmp/dst.zip",
            Capability::ArchivePackTo,
        )
        .unwrap_err();
        assert!(err.is(code::CAPABILITY_OFFLINE), "{:?}", err);
        // Both local → allowed.
        assert!(require_pair(
            "/tmp/src",
            Capability::ArchivePackFrom,
            "/tmp/dst.zip",
            Capability::ArchivePackTo
        )
        .is_ok());
    }
}
