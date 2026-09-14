//! Persist profile IDs and an endpoint digest, never remote URLs or credentials.
use super::types::RecoveryLocation;
use crate::errors::{code, AppError, AppResult};
use crate::net::session::{net_list_profiles, Profile};
use crate::vfs::{self, Scheme};
use sha2::{Digest, Sha256};
use std::path::Path;

fn invalid() -> AppError {
    AppError::new(code::VALIDATION_BAD_INPUT).op("transfer-journal")
}
fn identity(profile: &Profile) -> String {
    format!(
        "vfs1:{}",
        hex::encode(Sha256::digest(
            format!("{}\0{}", profile.protocol, profile.connection_key()).as_bytes(),
        ))
    )
}
fn legacy_identity(profile: &Profile) -> String {
    let host = profile
        .host
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(&profile.host)
        .trim_end_matches('/');
    hex::encode(Sha256::digest(
        format!(
            "{}\0{}@{}:{}",
            profile.protocol, profile.user, host, profile.port
        )
        .as_bytes(),
    ))
}

impl RecoveryLocation {
    pub fn capture(raw: &str) -> AppResult<Self> {
        Self::capture_with(raw, &net_list_profiles())
    }

    pub(super) fn capture_with(raw: &str, profiles: &[Profile]) -> AppResult<Self> {
        if raw.len() > 32_768 || raw.chars().any(char::is_control) {
            return Err(invalid());
        }
        let address = vfs::parse_address(raw).map_err(|_| invalid())?;
        if address.scheme() == Scheme::Local {
            let provider_path = address.path();
            let path = Path::new(&provider_path);
            if !path.is_absolute() {
                return Err(invalid());
            }
            return Ok(Self::Local {
                path: address.path(),
            });
        }
        let key = address.connection_key().ok_or_else(invalid)?;
        let profile = profiles
            .iter()
            .find(|p| p.scheme() == address.scheme().as_str() && p.connection_key() == key)
            .ok_or_else(invalid)?;
        Ok(Self::Remote {
            profile: profile.id.clone(),
            identity: identity(profile),
            path: address.path(),
        })
    }

    /// Resolves only through the current profile and an already verified session.
    /// Editing a profile's endpoint invalidates its old recovery records.
    pub fn resolve(&self) -> AppResult<String> {
        match self {
            Self::Local { path } if Path::new(path).is_absolute() => Ok(path.clone()),
            Self::Local { .. } => Err(invalid()),
            Self::Remote {
                profile: profile_id,
                identity: expected,
                path,
            } => {
                let profiles = net_list_profiles();
                let profile = profiles
                    .iter()
                    .find(|p| {
                        &p.id == profile_id
                            && (identity(p) == *expected || legacy_identity(p) == *expected)
                    })
                    .ok_or_else(invalid)?;
                if !path.starts_with('/') || path.chars().any(char::is_control) {
                    return Err(invalid());
                }
                let address =
                    vfs::location::remote_uri(profile.scheme(), &profile.connection_key(), path)?;
                vfs::resolve(&address)?;
                Ok(address)
            }
        }
    }

    pub fn with_path(&self, path: &Path) -> AppResult<Self> {
        let path = path.to_str().ok_or_else(invalid)?.to_owned();
        Ok(match self {
            Self::Local { .. } => Self::Local { path },
            Self::Remote {
                profile, identity, ..
            } => Self::Remote {
                profile: profile.clone(),
                identity: identity.clone(),
                path,
            },
        })
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    #[test]
    fn journal_capture_decodes_once_and_keeps_legacy_endpoint_digest_reproducible() {
        let p: Profile = serde_json::from_value(serde_json::json!({
            "id":"vfs-migration", "name":"test", "protocol":"sftp", "host":"Example.COM",
            "port":22, "user":"u", "auth":"agent", "key_path":null, "start_path":"/a%20b"
        }))
        .unwrap();
        let legacy = hex::encode(Sha256::digest(b"sftp\0u@Example.COM:22"));
        assert_eq!(legacy_identity(&p), legacy);
        assert!(identity(&p).starts_with("vfs1:"));
        let location =
            RecoveryLocation::capture_with(&p.address(), std::slice::from_ref(&p)).unwrap();
        let RecoveryLocation::Remote {
            path,
            identity: digest,
            ..
        } = location
        else {
            panic!("expected remote")
        };
        assert_eq!(path, "/a%20b");
        assert_eq!(digest, identity(&p));
        let old =
            serde_json::json!({"kind":"remote","profile":p.id,"identity":legacy,"path":"/a%20b"});
        let restored: RecoveryLocation = serde_json::from_value(old).unwrap();
        let RecoveryLocation::Remote { path, .. } = restored else {
            panic!("expected remote")
        };
        assert_eq!(path, "/a%20b");
    }
}
