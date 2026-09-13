//! Platform certificate validation plus optional profile-scoped certificate pinning.
use super::{certificate, session::Profile};
use crate::errors::{code, AppError, AppResult};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use suppaftp::rustls::{
    self,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
    DigitallySignedStruct, SignatureScheme,
};

#[derive(Clone, Debug, Default)]
pub struct TlsEvidence {
    pub failure: Arc<Mutex<Option<AppError>>>,
    pub fingerprint: Arc<Mutex<Option<String>>>,
}
impl TlsEvidence {
    pub fn error(&self, _fallback: impl std::fmt::Display) -> AppError {
        self.failure
            .lock()
            .ok()
            .and_then(|e| e.clone())
            .unwrap_or_else(|| AppError::new(code::NET_TLS_PROTOCOL).op("tls-handshake"))
    }
}

#[derive(Debug)]
struct VerifiedPin {
    platform: Arc<dyn ServerCertVerifier>,
    pin: Option<String>,
    evidence: TlsEvidence,
}
impl ServerCertVerifier for VerifiedPin {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        use x509_parser::prelude::FromDer;
        if let Ok((_, cert)) =
            x509_parser::certificate::X509Certificate::from_der(end_entity.as_ref())
        {
            let timestamp = now.as_secs() as i64;
            let invalid = if timestamp < cert.validity().not_before.timestamp() {
                Some(rustls::CertificateError::NotValidYet)
            } else if timestamp > cert.validity().not_after.timestamp() {
                Some(rustls::CertificateError::Expired)
            } else {
                None
            };
            if let Some(invalid) = invalid {
                let error = rustls::Error::InvalidCertificate(invalid);
                if let Ok(mut failure) = self.evidence.failure.lock() {
                    *failure = Some(classify(&error));
                }
                return Err(error);
            }
        }
        let fingerprint = hex::encode(Sha256::digest(end_entity.as_ref()));
        if self
            .pin
            .as_ref()
            .is_some_and(|expected| expected != &fingerprint)
        {
            if let Ok(mut failure) = self.evidence.failure.lock() {
                *failure = Some(AppError::new(code::NET_CERTIFICATE_CHANGED).op("tls-pin"));
            }
            return Err(rustls::Error::General(
                "profile certificate pin changed".into(),
            ));
        }
        let result =
            self.platform
                .verify_server_cert(end_entity, intermediates, server_name, ocsp, now);
        if let Err(error) = &result {
            if let Ok(mut failure) = self.evidence.failure.lock() {
                *failure = Some(classify(error));
            }
            return result;
        }
        if let Ok(mut current) = self.evidence.fingerprint.lock() {
            *current = Some(fingerprint);
        }
        result
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.platform
            .verify_tls12_signature(message, cert, signature)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.platform
            .verify_tls13_signature(message, cert, signature)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.platform.supported_verify_schemes()
    }
}

fn classify(error: &rustls::Error) -> AppError {
    use rustls::CertificateError as C;
    let code = match error {
        rustls::Error::InvalidCertificate(C::Expired | C::ExpiredContext { .. }) => {
            code::NET_CERTIFICATE_EXPIRED
        }
        rustls::Error::InvalidCertificate(C::NotValidYet | C::NotValidYetContext { .. }) => {
            code::NET_CERTIFICATE_FUTURE
        }
        rustls::Error::InvalidCertificate(
            C::NotValidForName | C::NotValidForNameContext { .. },
        ) => code::NET_CERTIFICATE_HOSTNAME,
        rustls::Error::InvalidCertificate(C::Revoked) => code::NET_CERTIFICATE_REVOKED,
        _ => code::NET_TLS,
    };
    AppError::new(code).op("tls-verification")
}

pub fn config(profile: &Profile) -> AppResult<(rustls::ClientConfig, TlsEvidence)> {
    let approved = certificate::approved_der(profile)?;
    #[allow(unused_mut)]
    let mut roots: Vec<_> = approved.into_iter().map(CertificateDer::from).collect();
    #[cfg(test)]
    for pem in super::tls::extra_ca_pems()? {
        let (_, pem) =
            x509_parser::pem::parse_x509_pem(&pem).map_err(|_| AppError::new(code::NET_TLS))?;
        roots.push(CertificateDer::from(pem.contents));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let platform =
        rustls_platform_verifier::Verifier::new_with_extra_roots(roots, provider.clone()).map_err(
            |error| {
                AppError::new(code::NET_TLS)
                    .op("platform-trust")
                    .detail(error)
            },
        )?;
    let evidence = TlsEvidence::default();
    let verifier = VerifiedPin {
        platform: Arc::new(platform),
        pin: profile
            .security
            .certificate
            .as_ref()
            .filter(|p| !p.is_ca)
            .map(|p| p.fingerprint.clone()),
        evidence: evidence.clone(),
    };
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .map_err(|error| AppError::new(code::NET_TLS).detail(error))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok((config, evidence))
}

#[cfg(test)]
mod tests;
