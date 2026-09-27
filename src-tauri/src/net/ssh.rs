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
