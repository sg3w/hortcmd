//! Preserve structured trash failures without guessing from localized diagnostics.
use super::{code, AppError, ErrorAction};

impl From<trash::Error> for AppError {
    fn from(error: trash::Error) -> Self {
        // Capture before consuming fields; the central detail builder redacts it.
        let detail = error.to_string();
        let mapped = match error {
            #[cfg(all(
                unix,
                not(target_os = "macos"),
                not(target_os = "ios"),
                not(target_os = "android")
            ))]
            trash::Error::FileSystem { source, .. } => AppError::from(source),
            // In trash 5.2.6 this is osascript's process exit status, NOT errno
            // or an AppleEvent error. Even exit 1 does not establish denial.
            #[cfg(target_os = "macos")]
            trash::Error::Os { .. } => AppError::new(code::TRASH_FINDER_FAILED),
            trash::Error::TargetedRoot => AppError::new(code::VALIDATION_BAD_INPUT),
            trash::Error::ConvertOsString { .. } => AppError::new(code::FS_INVALID_NAME),
            trash::Error::RestoreCollision { .. } => AppError::new(code::FS_EXISTS),
            // CouldNotAccess conflates a missing target and denied access;
            // CanonicalizePath loses its original io::Error too. Keep uncertain.
            _ => AppError::new(code::TRASH_FAILED).action(ErrorAction::Report),
        };
        mapped.op("trash").detail(detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_errors_never_claim_permission_denial_or_missing_file() {
        for error in [
            trash::Error::CouldNotAccess {
                target: "/private/file".into(),
            },
            trash::Error::CanonicalizePath {
                original: "/private/file".into(),
            },
            trash::Error::Unknown {
                description: "Permission denied (-1743) sftp://user:secret@host/file".into(),
            },
        ] {
            let mapped = AppError::from(error);
            assert_eq!(&*mapped.code, code::TRASH_FAILED);
            assert_eq!(mapped.op.as_deref(), Some("trash"));
            assert!(!mapped.detail.unwrap().contains("secret"));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn applescript_exit_status_is_not_errno_or_a_permission_verdict() {
        for status in [1, 2, 13, -1743] {
            let mapped = AppError::from(trash::Error::Os {
                code: status,
                description: "opaque native diagnostic".into(),
            });
            assert_eq!(&*mapped.code, code::TRASH_FINDER_FAILED);
            assert_eq!(mapped.action, None);
            assert!(mapped.detail.unwrap().contains("opaque native diagnostic"));
        }
    }

    #[cfg(all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    ))]
    #[test]
    fn structured_filesystem_causes_survive() {
        for (kind, expected) in [
            (std::io::ErrorKind::PermissionDenied, code::FS_PERMISSION),
            (std::io::ErrorKind::NotFound, code::FS_NOT_FOUND),
            (std::io::ErrorKind::Other, code::FS_IO),
        ] {
            let mapped = AppError::from(trash::Error::FileSystem {
                path: "/test".into(),
                source: std::io::Error::from(kind),
            });
            assert_eq!(&*mapped.code, expected);
        }
    }
}
