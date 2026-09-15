//! One transport policy shared by desktop and direct provider callers.
use super::session::Profile;
use crate::errors::{code, AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use ts_rs::TS;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ProfileSecurity {
    pub plaintext_acknowledged: bool,
    pub certificate: Option<super::certificate::CertificateTrust>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct TransportPolicy {
    pub allow_plaintext: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum TransportState {
    EncryptedVerified,
    EncryptedUserApproved,
    EncryptedUnverified,
    Plaintext,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum CertificateStatus {
    NotApplicable,
    ProfileApproved,
    SshHostKey,
    SessionAuthentication,
    PlatformVerified,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct TransportInfo {
    pub profile_id: String,
    pub fingerprint: Option<String>,
    pub state: TransportState,
    pub protocol: Option<String>,
    pub cipher: Option<String>,
    pub certificate_status: CertificateStatus,
    pub signing: Option<bool>,
    pub encryption: bool,
}

fn policy_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[tauri::command]
pub fn net_transport_policy() -> AppResult<TransportPolicy> {
    let path = crate::config::path("transport-policy.json")?;
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(AppError::from),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(TransportPolicy::default())
        }
        Err(error) => Err(error.into()),
    }
}

#[tauri::command]
pub fn net_set_transport_policy(policy: TransportPolicy) -> AppResult<()> {
    use std::io::Write;
    let _guard = policy_lock()
        .lock()
        .map_err(|_| AppError::new(code::FS_IO))?;
    let path = crate::config::path("transport-policy.json")?;
    let parent = path.parent().ok_or_else(|| AppError::new(code::FS_IO))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".transport-policy-{}", super::unique_id()));
    let result = (|| -> AppResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(&policy)?)?;
        file.sync_all()?;
        std::fs::rename(&temporary, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Parse scheme/host centrally; alternate schemes and URL userinfo are not hostnames.
pub fn webdav_endpoint(profile: &Profile) -> AppResult<(&str, &str)> {
    let (scheme, host) = profile
        .host
        .split_once("://")
        .unwrap_or(("https", &profile.host));
    let host = host.trim_end_matches('/');
    if !matches!(scheme, "https" | "http")
        || host.is_empty()
        || host.contains(['/', '@', '?', '#'])
        || host.chars().any(char::is_control)
    {
        return Err(AppError::new(code::VALIDATION_BAD_INPUT).op("transport-policy"));
    }
    Ok((scheme, host))
}

pub fn check_with(profile: &Profile, policy: &TransportPolicy) -> AppResult<()> {
    let plaintext = match profile.protocol.as_str() {
        "ftp" => true,
        "webdav" => webdav_endpoint(profile)?.0 == "http",
        "sftp" | "ftps" | "ftps-implicit" | "smb" => false,
        _ => return Err(AppError::new(code::VALIDATION_BAD_INPUT).op("transport-policy")),
    };
    if plaintext && !(policy.allow_plaintext && profile.security.plaintext_acknowledged) {
        return Err(AppError::new(code::NET_PLAINTEXT_BLOCKED).op("transport-policy"));
    }
    Ok(())
}

pub fn check(profile: &Profile) -> AppResult<()> {
    if super::session::net_list_profiles().iter().any(|current| {
        current.id == profile.id
            && (current.security != profile.security
                || current.protocol != profile.protocol
                || current.connection_key() != profile.connection_key())
    }) {
        return Err(AppError::new(code::NET_CERTIFICATE_CHANGED).op("profile-changed"));
    }
    check_with(profile, &net_transport_policy()?)
}

impl TransportInfo {
    pub fn connected(profile: &Profile, encrypted: bool) -> Self {
        Self {
            profile_id: profile.id.clone(),
            fingerprint: None,
            state: if !encrypted {
                TransportState::Plaintext
            } else if profile.security.certificate.is_some() {
                TransportState::EncryptedUserApproved
            } else {
                TransportState::EncryptedVerified
            },
            protocol: None,
            cipher: None,
            certificate_status: if !encrypted {
                CertificateStatus::NotApplicable
            } else if profile.security.certificate.is_some() {
                CertificateStatus::ProfileApproved
            } else if profile.protocol == "sftp" {
                CertificateStatus::SshHostKey
            } else if profile.protocol == "smb" {
                CertificateStatus::SessionAuthentication
            } else {
                CertificateStatus::PlatformVerified
            },
            signing: None,
            encryption: encrypted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile(protocol: &str, host: &str) -> Profile {
        Profile {
            monitor: None,
            security: ProfileSecurity::default(),
            id: "profile".into(),
            name: "Example".into(),
            protocol: protocol.into(),
            host: host.into(),
            port: 443,
            user: "alice".into(),
            auth: "password".into(),
            key_path: None,
            start_path: "/".into(),
        }
    }
    #[test]
    fn plaintext_requires_both_explicit_gates_for_every_attempt() {
        for (protocol, host) in [("ftp", "example.test"), ("webdav", "http://example.test")] {
            let mut profile = profile(protocol, host);
            let mut policy = TransportPolicy::default();
            assert!(check_with(&profile, &policy).is_err());
            policy.allow_plaintext = true;
            assert!(check_with(&profile, &policy).is_err());
            profile.security.plaintext_acknowledged = true;
            assert!(check_with(&profile, &policy).is_ok());
            policy.allow_plaintext = false;
            assert!(check_with(&profile, &policy).is_err());
        }
    }
    #[test]
    fn https_is_default_even_on_a_nonstandard_port_and_schemes_cannot_downgrade() {
        let mut profile = profile("webdav", "example.test");
        profile.port = 80;
        assert_eq!(
            webdav_endpoint(&profile).unwrap(),
            ("https", "example.test")
        );
        assert!(check_with(&profile, &TransportPolicy::default()).is_ok());
        for host in [
            "ftp://example.test",
            "https://alice:secret@example.test",
            "https://example.test/path",
            "https://example.test?token=value",
        ] {
            profile.host = host.into();
            assert!(check_with(&profile, &TransportPolicy::default()).is_err());
        }
    }
    #[test]
    fn existing_profiles_deserialize_without_implicitly_accepting_plaintext() {
        let profile: Profile = serde_json::from_value(serde_json::json!({
            "id":"legacy", "name":"Legacy", "protocol":"ftp", "host":"example.test", "port":21,
            "user":"alice", "auth":"password", "key_path":null, "start_path":"/"
        }))
        .unwrap();
        assert!(!profile.security.plaintext_acknowledged);
        assert!(check_with(
            &profile,
            &TransportPolicy {
                allow_plaintext: true
            }
        )
        .is_err());
    }
}
