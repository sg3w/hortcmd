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
use russh::client::{self, AuthResult, Msg};
use russh::Channel;
use std::sync::{Arc, Mutex};

/// russh client handler. Its only job is server authentication: the
/// presented host key is checked against the trust stores (ticket 028)
/// and the verdict recorded, because russh itself only reports a
/// generic handshake failure when the check says no.
pub struct ClientHandler {
    id: HostId,
    verdict: Arc<Mutex<Option<Result<Decision, String>>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        let verdict = hostkey::check(&self.id, server_public_key);
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
        let verdict: Arc<Mutex<Option<Result<Decision, String>>>> = Arc::new(Mutex::new(None));
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
                    Ok(_) => format!("Host-Schlüssel von {} wurde nicht geprüft", id.host),
                    Err(e) => format!("Verbindung zu {} fehlgeschlagen: {}", profile.host, e),
                }))
            }
        }

        let mut handle = connected
            .map_err(|e| format!("Verbindung zu {} fehlgeschlagen: {}", profile.host, e))?;
        authenticate(&mut handle, profile, secret).await?;
        Ok(SshSession { handle })
    }

    /// Opens a new session channel on this connection.
    /// Whether the transport is gone (the peer closed it, or it broke).
    /// russh flags this on its handle, so the check is a read of local
    /// state — no round trip — which is what makes it usable from the
    /// status refresh (ticket 032).
    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    pub async fn open_channel(&self) -> Result<Channel<Msg>, String> {
        self.handle
            .channel_open_session()
            .await
            .map_err(|e| format!("SSH-Kanal fehlgeschlagen: {}", e))
    }
}

async fn authenticate(
    handle: &mut client::Handle<ClientHandler>,
    profile: &Profile,
    secret: Option<&str>,
) -> Result<(), String> {
    let user = profile.user.as_str();
    let result = match profile.auth.as_str() {
        "password" => {
            let pw = secret.ok_or("Passwort fehlt")?;
            handle
                .authenticate_password(user, pw)
                .await
                .map_err(|e| e.to_string())?
        }
        "anonymous" => handle
            .authenticate_none(user)
            .await
            .map_err(|e| e.to_string())?,
        "key" => {
            let path = profile
                .key_path
                .as_ref()
                .ok_or("Kein Schlüsselpfad angegeben")?;
            let key = russh::keys::load_secret_key(path, secret)
                .map_err(|e| format!("Schlüssel laden fehlgeschlagen: {}", e))?;
            let with = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None);
            handle
                .authenticate_publickey(user, with)
                .await
                .map_err(|e| e.to_string())?
        }
        "agent" => return authenticate_agent(handle, user).await,
        other => return Err(format!("Unbekannte Auth-Methode: {}", other)),
    };

    match result {
        AuthResult::Success => Ok(()),
        AuthResult::Failure { .. } => Err("Authentifizierung fehlgeschlagen".to_string()),
    }
}

/// Authenticates via ssh-agent using the user's existing keys (no
/// private key is ever stored by us).
async fn authenticate_agent(
    handle: &mut client::Handle<ClientHandler>,
    user: &str,
) -> Result<(), String> {
    let mut agent = russh::keys::agent::client::AgentClient::connect_env()
        .await
        .map_err(|e| format!("ssh-agent nicht erreichbar: {}", e))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| e.to_string())?;
    if identities.is_empty() {
        return Err("ssh-agent bietet keine Schlüssel an".to_string());
    }
    for key in identities {
        let result = handle
            .authenticate_publickey_with(user, key, None, &mut agent)
            .await
            .map_err(|e| e.to_string())?;
        if matches!(result, AuthResult::Success) {
            return Ok(());
        }
    }
    Err("Kein ssh-agent-Schlüssel wurde akzeptiert".to_string())
}
