// ============================================================
// SSH transport (tickets 022/026).
//
// One authenticated SSH connection, shared by everything that rides on
// it: the SFTP provider opens the `sftp` subsystem on it (ticket 022),
// the terminal opens an interactive shell channel (ticket 026) — so the
// single host-key check in `connect` (ticket 028, `net::hostkey`)
// protects both. The russh client is async, so callers drive it through
// the shared Tokio runtime in `net::session`.
// ============================================================

use super::hostkey::{self, ConnectError, Decision, HostId};
use super::session::Profile;
use crate::errors::{code, AppError, AppResult, ErrorAction};
use russh::client::{self, AuthResult, Msg};
use russh::Channel;
use std::sync::{Arc, Mutex};

/// russh client handler. Its only job is server authentication: the
/// presented host key is checked against the trust stores (ticket 028)
/// and the verdict recorded, because russh itself only reports a
/// generic handshake failure when the check says no.
pub struct ClientHandler {
    id: HostId,
    verdict: Arc<Mutex<Option<AppResult<Decision>>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // A host certificate would have to be checked against a CA, and
        // hortcmd's trust stores hold host keys. russh 0.53 never offered
        // this callback a certificate at all, so refusing one keeps the
        // guarantee the trust stores actually make instead of widening it.
        let verdict = match server_public_key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => {
                hostkey::check(&self.id, key)
            }
            russh::keys::PublicKeyOrCertificate::Certificate(_) => {
                Err(AppError::new(code::SECURITY_HOST_KEY_UNKNOWN)
                    .with("host", &self.id.host)
                    .provider("sftp")
                    .detail("the server presented a host certificate, which is not trusted"))
            }
        };
        // Only a host we already trust may continue; everything else
        // aborts the handshake *before* any authentication method runs.
        let trusted = matches!(verdict, Ok(Decision::Trusted));
        *self.verdict.lock().unwrap() = Some(verdict);
        Ok(trusted)
    }
}

/// A live, authenticated SSH connection. Dropping it closes the
/// connection and every channel opened on it.
pub struct SshSession {
    handle: client::Handle<ClientHandler>,
}

impl SshSession {
    /// Connects and authenticates against the profile's host. The host
    /// identity is verified first; an unknown or changed key returns
    /// `ConnectError::HostKey` and no secret ever leaves the machine.
    pub async fn connect(profile: &Profile, secret: Option<&str>) -> Result<Self, ConnectError> {
        let id = HostId::new(&profile.host, profile.port);
        let verdict: Arc<Mutex<Option<AppResult<Decision>>>> = Arc::new(Mutex::new(None));
        let handler = ClientHandler {
            id: id.clone(),
            verdict: Arc::clone(&verdict),
        };
        let config = Arc::new(client::Config::default());
        let connected =
            client::connect(config, (profile.host.as_str(), profile.port), handler).await;

        // The recorded verdict, not the russh error, decides what the
        // user is told — and a handshake that never reached the host key
        // is a failure, so no path here can authenticate unverified.
        let checked = verdict.lock().unwrap().take();
        match checked {
            Some(Ok(Decision::Trusted)) => {}
            Some(Ok(Decision::Challenge(challenge))) => {
                return Err(ConnectError::HostKey(Box::new(challenge)))
            }
            Some(Err(message)) => return Err(ConnectError::Message(message)),
            None => {
                return Err(ConnectError::Message(match connected {
                    Ok(_) => AppError::new(code::SECURITY_HOST_KEY_UNKNOWN)
                        .with("host", &id.host)
                        .action(ErrorAction::ConfirmHostKey),
                    Err(e) => AppError::new(code::NET_UNREACHABLE)
                        .with("host", &profile.host)
                        .provider("sftp")
                        .action(ErrorAction::Retry)
                        .detail(e),
                }))
            }
        }

        let mut handle = connected.map_err(|e| {
            AppError::from(format!(
                "Verbindung zu {} fehlgeschlagen: {}",
                profile.host, e
            ))
        })?;
        authenticate(&mut handle, profile, secret).await?;
        Ok(SshSession { handle })
    }

    /// Opens a new session channel on this connection.
    /// Whether the transport is gone (the peer closed it, or it broke).
    /// russh flags this on its handle, so the check is a read of local
    /// state — no round trip — which is what makes it usable from the
    /// status refresh (ticket 032).
    pub async fn keepalive(&self) -> Result<(), russh::Error> {
        self.handle.send_keepalive(true).await
    }

    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    pub async fn open_channel(&self) -> AppResult<Channel<Msg>> {
        self.handle
            .channel_open_session()
            .await
            .map_err(|e| AppError::from(format!("SSH-Kanal fehlgeschlagen: {}", e)))
    }
}

async fn authenticate(
    handle: &mut client::Handle<ClientHandler>,
    profile: &Profile,
    secret: Option<&str>,
) -> AppResult<()> {
    let user = profile.user.as_str();
    let result = match profile.auth.as_str() {
        "password" => {
            let pw = secret.ok_or("Passwort fehlt")?;
            handle
                .authenticate_password(user, pw)
                .await
                .map_err(AppError::from)?
        }
        "anonymous" => handle
            .authenticate_none(user)
            .await
            .map_err(AppError::from)?,
        "key" => {
            let path = profile
                .key_path
                .as_ref()
                .ok_or("Kein Schlüsselpfad angegeben")?;
            let key = russh::keys::load_secret_key(path, secret)
                .map_err(|e| AppError::from(format!("Schlüssel laden fehlgeschlagen: {}", e)))?;
            // An RSA key is signed by AWS-LC, not by the `rsa` crate.
            if matches!(key.algorithm(), russh::keys::Algorithm::Rsa { .. }) {
                return authenticate_rsa(handle, user, &key).await;
            }
            let with = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None);
            handle
                .authenticate_publickey(user, with)
                .await
                .map_err(AppError::from)?
        }
        "agent" => return authenticate_agent(handle, user).await,
        other => {
            return Err(AppError::from(format!(
                "Unbekannte Auth-Methode: {}",
                other
            )))
        }
    };

    match result {
        AuthResult::Success => Ok(()),
        AuthResult::Failure { .. } => Err(AppError::new(code::NET_AUTH)
            .provider("sftp")
            .action(ErrorAction::Authenticate)),
    }
}

/// Authenticates via ssh-agent using the user's existing keys (no
/// private key is ever stored by us).
async fn authenticate_agent(
    handle: &mut client::Handle<ClientHandler>,
    user: &str,
) -> AppResult<()> {
    let mut agent = russh::keys::agent::client::AgentClient::connect_env()
        .await
        .map_err(|e| AppError::from(format!("ssh-agent nicht erreichbar: {}", e)))?;
    let identities = agent.request_identities().await.map_err(AppError::from)?;
    if identities.is_empty() {
        return Err(AppError::new(code::NET_AUTH)
            .provider("sftp")
            .action(ErrorAction::Authenticate)
            .detail("ssh-agent offers no keys"));
    }
    for identity in identities {
        // Only plain public keys are offered: a certificate identity needs a
        // CA-aware method, which this client does not use (see `check_server_key`).
        let russh::keys::agent::AgentIdentity::PublicKey { key, .. } = identity else {
            continue;
        };
        let result = handle
            .authenticate_publickey_with(user, key, None, &mut agent)
            .await
            .map_err(|e| AppError::new(code::NET_AUTH).detail(e))?;
        if matches!(result, AuthResult::Success) {
            return Ok(());
        }
    }
    Err(AppError::from("Kein ssh-agent-Schlüssel wurde akzeptiert"))
}

// ---------- RSA public-key authentication (ticket 380) ----------

/// The RSA signature algorithms hortcmd offers, strongest first.
///
/// `ssh-rsa` (SHA-1) is deliberately absent. AWS-LC does not sign with
/// SHA-1, and OpenSSH has refused that algorithm by default since 8.8
/// (2021), so offering it would mean keeping a weak signature for
/// servers that reject it anyway.
const RSA_HASHES: [russh::keys::HashAlg; 2] =
    [russh::keys::HashAlg::Sha512, russh::keys::HashAlg::Sha256];

/// A PKCS#8 copy of a private key, wiped when it goes out of scope.
struct KeyBytes(Vec<u8>);

impl Drop for KeyBytes {
    fn drop(&mut self) {
        self.0.fill(0);
        // Keep the write: the buffer is dead afterwards, and a `fill`
        // whose result nobody reads is exactly what a compiler may drop.
        std::hint::black_box(&self.0);
    }
}

/// Signs an RSA public-key authentication request with AWS-LC.
///
/// The `rsa` crate has no constant-time private-key path
/// (RUSTSEC-2023-0071, the Marvin attack), and SSH authentication over a
/// network is exactly the observable setting that advisory names. So the
/// signature is produced by AWS-LC's blinded RSA instead — the same
/// AWS-LC russh already uses for its other primitives. What is left for
/// the `rsa` crate is parsing the key file and verifying a server's RSA
/// host key: neither can leak a private key (ticket 380).
struct RsaSigner {
    key: aws_lc_rs::signature::RsaKeyPair,
}

impl RsaSigner {
    /// AWS-LC takes a PKCS#8 key, which is the one encoding russh can
    /// produce from a loaded OpenSSH key. The intermediate copy is wiped
    /// as soon as AWS-LC has taken ownership of the key material.
    fn new(key: &russh::keys::PrivateKey) -> AppResult<Self> {
        let der = KeyBytes(russh::keys::pkcs8::encode_pkcs8(key).map_err(|e| {
            AppError::new(code::NET_AUTH)
                .provider("sftp")
                .op("ssh-rsa-key")
                .detail(e)
        })?);
        let key = aws_lc_rs::signature::RsaKeyPair::from_pkcs8(&der.0).map_err(|e| {
            AppError::new(code::NET_AUTH)
                .provider("sftp")
                .op("ssh-rsa-key")
                .detail(e)
        })?;
        Ok(Self { key })
    }
}

/// What can go wrong while signing. `Signer::Error` has to absorb
/// russh's send failure, so the two stay apart and are joined again at
/// the call site.
enum SignError {
    Send,
    Sign(AppError),
}

impl From<russh::SendError> for SignError {
    fn from(_: russh::SendError) -> Self {
        Self::Send
    }
}

impl From<SignError> for AppError {
    fn from(error: SignError) -> Self {
        match error {
            SignError::Send => AppError::new(code::NET_LOST).provider("sftp"),
            SignError::Sign(error) => error,
        }
    }
}

/// The signature blob for one `to_sign` buffer, as russh expects it:
/// the buffer itself, then the signature as an SSH string pair — the
/// same shape an ssh-agent returns.
fn sign_rsa(
    key: &aws_lc_rs::signature::RsaKeyPair,
    hash: russh::keys::HashAlg,
    to_sign: Vec<u8>,
) -> AppResult<Vec<u8>> {
    use aws_lc_rs::signature::{RsaEncoding, RSA_PKCS1_SHA256, RSA_PKCS1_SHA512};
    use russh::keys::ssh_encoding::Encode;

    let (padding, algorithm): (&'static dyn RsaEncoding, &str) = match hash {
        russh::keys::HashAlg::Sha256 => (&RSA_PKCS1_SHA256, "rsa-sha2-256"),
        russh::keys::HashAlg::Sha512 => (&RSA_PKCS1_SHA512, "rsa-sha2-512"),
        other => {
            return Err(AppError::new(code::NET_AUTH)
                .provider("sftp")
                .op("ssh-rsa-sign")
                .detail(format!("unsupported RSA hash {:?}", other)))
        }
    };
    let mut signature = vec![0u8; key.public_modulus_len()];
    key.sign(
        padding,
        &aws_lc_rs::rand::SystemRandom::new(),
        &to_sign,
        &mut signature,
    )
    .map_err(|e| {
        AppError::new(code::NET_AUTH)
            .provider("sftp")
            .op("ssh-rsa-sign")
            .detail(e)
    })?;

    let mut out = to_sign;
    let encode = |out: &mut Vec<u8>| -> Result<(), russh::keys::ssh_encoding::Error> {
        (algorithm.len() + signature.len() + 8).encode(out)?;
        algorithm.encode(out)?;
        signature.as_slice().encode(out)
    };
    encode(&mut out).map_err(|e| {
        AppError::new(code::NET_AUTH)
            .provider("sftp")
            .op("ssh-rsa-sign")
            .detail(e)
    })?;
    Ok(out)
}

impl russh::Signer for RsaSigner {
    type Error = SignError;

    async fn auth_sign(
        &mut self,
        _identity: &russh::keys::agent::AgentIdentity,
        hash_alg: Option<russh::keys::HashAlg>,
        to_sign: Vec<u8>,
    ) -> Result<Vec<u8>, Self::Error> {
        let hash = hash_alg.ok_or_else(|| {
            SignError::Sign(
                AppError::new(code::NET_AUTH)
                    .provider("sftp")
                    .op("ssh-rsa-sign")
                    .detail("RSA signing needs rsa-sha2-256 or rsa-sha2-512"),
            )
        })?;
        sign_rsa(&self.key, hash, to_sign).map_err(SignError::Sign)
    }
}

/// Public-key authentication with an RSA key.
///
/// The strongest rsa-sha2 variant is offered first, and the weaker one
/// only if the server refuses it — a server that accepts neither gets
/// the same authentication error any other refused key gets.
async fn authenticate_rsa(
    handle: &mut client::Handle<ClientHandler>,
    user: &str,
    key: &russh::keys::PrivateKey,
) -> AppResult<()> {
    let public = key.public_key().clone();
    for hash in RSA_HASHES {
        let mut signer = RsaSigner::new(key)?;
        let result = handle
            .authenticate_publickey_with(user, public.clone(), Some(hash), &mut signer)
            .await
            .map_err(AppError::from)?;
        if matches!(result, AuthResult::Success) {
            return Ok(());
        }
    }
    Err(AppError::new(code::NET_AUTH)
        .provider("sftp")
        .action(ErrorAction::Authenticate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::encoding::AsDer;
    use russh::keys::ssh_encoding::Decode;
    use russh::keys::{Algorithm, HashAlg};

    /// A throwaway RSA key, generated by AWS-LC itself so the test stays
    /// fast: RSA key generation in the `rsa` crate's debug build is not.
    fn test_key() -> russh::keys::PrivateKey {
        let pair = aws_lc_rs::rsa::KeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap();
        let der = AsDer::<aws_lc_rs::encoding::Pkcs8V1Der>::as_der(&pair).unwrap();
        russh::keys::pkcs8::decode_pkcs8(der.as_ref(), None).unwrap()
    }

    /// The signature AWS-LC produces is the one an SSH server verifies:
    /// same encoding as an agent's answer, same algorithm name, and it
    /// verifies against the key's own public half (ticket 380).
    #[test]
    fn rsa_signatures_verify_and_carry_the_negotiated_algorithm() {
        let key = test_key();
        assert!(matches!(key.algorithm(), Algorithm::Rsa { .. }));
        let signer = RsaSigner::new(&key).expect("build the AWS-LC signer");
        let data = b"session-id and userauth request".to_vec();

        for (hash, name) in [
            (HashAlg::Sha512, "rsa-sha2-512"),
            (HashAlg::Sha256, "rsa-sha2-256"),
        ] {
            let out = sign_rsa(&signer.key, hash, data.clone()).expect("sign");
            // The buffer is returned unchanged, with the signature appended.
            assert_eq!(&out[..data.len()], &data[..]);

            let mut rest = &out[data.len()..];
            let inner = Vec::<u8>::decode(&mut rest).expect("signature field");
            assert!(rest.is_empty(), "trailing bytes after the signature");
            let mut inner = &inner[..];
            let algorithm = String::decode(&mut inner).expect("algorithm name");
            assert_eq!(algorithm, name);
            let blob = Vec::<u8>::decode(&mut inner).expect("signature blob");
            assert!(inner.is_empty());

            let signature =
                russh::keys::ssh_key::Signature::new(Algorithm::Rsa { hash: Some(hash) }, blob)
                    .expect("signature");
            russh::keys::signature::Verifier::verify(
                key.public_key().key_data(),
                &data,
                &signature,
            )
            .unwrap_or_else(|e| panic!("{} did not verify: {}", name, e));
        }
    }

    /// SHA-1 is not offered, and a missing hash is refused rather than
    /// silently signed with one the server did not negotiate.
    #[test]
    fn rsa_signing_refuses_an_algorithm_it_did_not_negotiate() {
        let key = test_key();
        let mut signer = RsaSigner::new(&key).unwrap();
        assert_eq!(RSA_HASHES, [HashAlg::Sha512, HashAlg::Sha256]);
        let identity = russh::keys::agent::AgentIdentity::PublicKey {
            key: key.public_key().clone(),
            comment: String::new(),
        };
        let error: AppError = crate::net::session::runtime()
            .block_on(russh::Signer::auth_sign(
                &mut signer,
                &identity,
                None,
                b"data".to_vec(),
            ))
            .expect_err("no hash, no signature")
            .into();
        assert!(error.is(code::NET_AUTH));
    }
}
