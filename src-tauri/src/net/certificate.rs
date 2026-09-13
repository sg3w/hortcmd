//! Profile-scoped certificate approval. File changes invalidate the saved approval.
use super::session::Profile;
use crate::errors::{code, AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use ts_rs::TS;
use x509_parser::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CertificateTrust {
    pub path: String,
    pub fingerprint: String,
    pub profile_id: String,
    pub endpoint: String,
    pub is_ca: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CertificatePreview {
    pub trust: CertificateTrust,
    pub subject: String,
    pub issuer: String,
    pub valid_from: String,
    pub valid_until: String,
    pub hostnames: Vec<String>,
    pub algorithm: String,
}

pub fn endpoint(profile: &Profile) -> String {
    hex::encode(Sha256::digest(
        format!(
            "{}\0{}\0{}",
            profile.id,
            profile.protocol,
            profile.connection_key()
        )
        .as_bytes(),
    ))
}

fn invalid() -> AppError {
    AppError::new(code::NET_CERTIFICATE_INVALID).op("certificate-trust")
}

fn read_der(path: &str) -> AppResult<Vec<u8>> {
    if !std::path::Path::new(path).is_absolute() {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 256 * 1024 {
        return Err(invalid());
    }
    if bytes.starts_with(b"-----BEGIN CERTIFICATE-----") {
        let (rest, pem) = x509_parser::pem::parse_x509_pem(&bytes).map_err(|_| invalid())?;
        if rest.iter().any(|b| !b.is_ascii_whitespace()) || pem.label != "CERTIFICATE" {
            return Err(invalid());
        }
        Ok(pem.contents)
    } else {
        Ok(bytes)
    }
}

#[tauri::command]
pub fn net_certificate_preview(profile: Profile, path: String) -> AppResult<CertificatePreview> {
    if !matches!(
        profile.protocol.as_str(),
        "ftps" | "ftps-implicit" | "webdav"
    ) {
        return Err(invalid());
    }
    let der = read_der(&path)?;
    let (rest, cert) = X509Certificate::from_der(&der).map_err(|_| invalid())?;
    if !rest.is_empty() {
        return Err(invalid());
    }
    let hostnames = cert
        .subject_alternative_name()
        .map_err(|_| invalid())?
        .map(|extension| {
            extension
                .value
                .general_names
                .iter()
                .filter_map(|name| match name {
                    x509_parser::extensions::GeneralName::DNSName(name) => Some((*name).to_owned()),
                    x509_parser::extensions::GeneralName::IPAddress(bytes) => match bytes.len() {
                        4 => <[u8; 4]>::try_from(*bytes)
                            .ok()
                            .map(std::net::Ipv4Addr::from)
                            .map(|ip| ip.to_string()),
                        16 => <[u8; 16]>::try_from(*bytes)
                            .ok()
                            .map(std::net::Ipv6Addr::from)
                            .map(|ip| ip.to_string()),
                        _ => None,
                    },
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(CertificatePreview {
        trust: CertificateTrust {
            path,
            fingerprint: hex::encode(Sha256::digest(&der)),
            profile_id: profile.id.clone(),
            endpoint: endpoint(&profile),
            is_ca: cert.is_ca(),
        },
        subject: cert.subject().to_string(),
        issuer: cert.issuer().to_string(),
        valid_from: cert.validity().not_before.to_string(),
        valid_until: cert.validity().not_after.to_string(),
        hostnames,
        algorithm: cert.signature_algorithm.algorithm.to_id_string(),
    })
}

fn changed() -> AppError {
    AppError::new(code::NET_CERTIFICATE_CHANGED).op("certificate-trust")
}

pub fn approved_der(profile: &Profile) -> AppResult<Option<Vec<u8>>> {
    let Some(trust) = &profile.security.certificate else {
        return Ok(None);
    };
    if trust.profile_id != profile.id || trust.endpoint != endpoint(profile) {
        return Err(changed());
    }
    let preview = net_certificate_preview(profile.clone(), trust.path.clone())?;
    if preview.trust.fingerprint != trust.fingerprint || preview.trust.is_ca != trust.is_ca {
        return Err(changed());
    }
    // Read again and compare so a file changed between preview and use cannot be trusted.
    let der = read_der(&trust.path)?;
    if hex::encode(Sha256::digest(&der)) != trust.fingerprint {
        return Err(changed());
    }
    Ok(Some(der))
}
