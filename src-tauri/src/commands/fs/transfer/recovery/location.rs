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
    hex::encode(Sha256::digest(
        format!("{}\0{}", profile.protocol, profile.connection_key()).as_bytes(),
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
        if address.scheme == Scheme::Local {
            let path = Path::new(&address.path);
            if !path.is_absolute() {
                return Err(invalid());
            }
            return Ok(Self::Local { path: address.path });
        }
        // VFS locations have no URI query/fragment or password component.
        let authority = raw
            .split_once("://")
            .ok_or_else(invalid)?
            .1
            .split('/')
            .next()
            .ok_or_else(invalid)?;
        if raw.contains(['?', '#'])
            || authority.contains('%')
            || address
                .user
                .as_ref()
                .is_some_and(|user| user.contains([':', '@']))
        {
            return Err(invalid());
        }
        let key = address.connection_key().ok_or_else(invalid)?;
        let profile = profiles
            .iter()
            .find(|p| p.scheme() == address.scheme.as_str() && p.connection_key() == key)
            .ok_or_else(invalid)?;
        Ok(Self::Remote {
            profile: profile.id.clone(),
            identity: identity(profile),
            path: address.path,
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
                    .find(|p| &p.id == profile_id && identity(p) == *expected)
                    .ok_or_else(invalid)?;
                if !path.starts_with('/') || path.chars().any(char::is_control) {
                    return Err(invalid());
                }
                let address = format!(
                    "{}://{}{}",
                    profile.scheme(),
                    profile.connection_key(),
                    path
                );
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
