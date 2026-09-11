// ============================================================
// SSH transport (tickets 022/026).
//
// One authenticated SSH connection, shared by everything that rides on
// it: the SFTP provider opens the `sftp` subsystem on it (ticket 022),
// the terminal opens an interactive shell channel (ticket 026). The
// russh client is async, so callers drive it through the shared Tokio
// runtime in `net::session`.
// ============================================================

use super::session::Profile;
use russh::client::{self, AuthResult, Msg};
use russh::Channel;
use std::sync::Arc;

/// Minimal russh client handler. Server-key verification is currently
/// trust-on-first-use (accept); known_hosts checking is future work.
pub struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// A live, authenticated SSH connection. Dropping it closes the
/// connection and every channel opened on it.
pub struct SshSession {
    handle: client::Handle<ClientHandler>,
}

impl SshSession {
    /// Connects and authenticates against the profile's host.
    pub async fn connect(profile: &Profile, secret: Option<&str>) -> Result<Self, String> {
        let config = Arc::new(client::Config::default());
        let mut handle =
            client::connect(config, (profile.host.as_str(), profile.port), ClientHandler)
                .await
                .map_err(|e| format!("Verbindung zu {} fehlgeschlagen: {}", profile.host, e))?;

        authenticate(&mut handle, profile, secret).await?;
        Ok(SshSession { handle })
    }

    /// Opens a new session channel on this connection.
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
