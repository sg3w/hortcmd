//! A stdio client bridge. It owns no filesystem or remote credentials.
use std::io::{BufRead, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("hortcmd-mcp: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Error> {
    let port = std::env::var("HORTCMD_MCP_PORT")
        .unwrap_or_else(|_| "8712".into())
        .parse::<u16>()?;
    let token = std::env::var("HORTCMD_MCP_TOKEN")
        .map_err(|_| "Set HORTCMD_MCP_TOKEN to the token shown in hortcmd's MCP settings.")?;
    if token.trim().is_empty() {
        return Err("HORTCMD_MCP_TOKEN must not be empty.".into());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(180))
        .build()?;
    let endpoint = format!("http://127.0.0.1:{port}/mcp");
    // Probe at startup, before waiting for stdin, so a disabled app never hangs.
    let probe = client
        .get(&endpoint)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|_| {
            "Cannot reach hortcmd. Start the app, enable MCP, and check HORTCMD_MCP_PORT."
        })?;
    if probe.status() != reqwest::StatusCode::METHOD_NOT_ALLOWED {
        return Err(
            "hortcmd refused the connection. Check the MCP token and port in settings.".into(),
        );
    }
    let session = Arc::new(Mutex::new(None::<String>));
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        loop {
            let mut line = String::new();
            let result = (&mut stdin).take(1024 * 1024 + 1).read_line(&mut line);
            match result {
                Ok(0) => break,
                Ok(_) if line.len() <= 1024 * 1024 => {
                    if tx.blocking_send(Ok(line)).is_err() {
                        break;
                    }
                }
                _ => {
                    let _ = tx.blocking_send(Err("Invalid or oversized stdio message."));
                    break;
                }
            }
        }
    });
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            Some(result) = tasks.join_next(), if !tasks.is_empty() => { result??; }
            line = rx.recv(), if tasks.len() < 8 => {
                let Some(line) = line else { break; };
                let value: serde_json::Value = serde_json::from_str(&line?)?;
                let initialize = value["method"] == "initialize";
                let work = forward(client.clone(), endpoint.clone(), token.clone(), session.clone(), value);
                if initialize { work.await?; } else { tasks.spawn(work); }
            }
        }
    }
    while let Some(result) = tasks.join_next().await {
        result??;
    }
    let id = session.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(id) = id {
        let _ = client
            .delete(endpoint)
            .bearer_auth(token)
            .header("mcp-session-id", id)
            .send()
            .await;
    }
    Ok(())
}

async fn forward(
    client: reqwest::Client,
    endpoint: String,
    token: String,
    session: Arc<Mutex<Option<String>>>,
    value: serde_json::Value,
) -> Result<(), Error> {
    let mut request = client
        .post(endpoint)
        .bearer_auth(token)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18");
    let id = session.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(id) = id {
        request = request.header("mcp-session-id", id);
    }
    let mut response = request.body(value.to_string()).send().await.map_err(|_| {
        "Connection to hortcmd failed or timed out. Check whether the app is still running."
    })?;
    if !response.status().is_success() {
        return Err(format!(
            "hortcmd returned HTTP {}. Check the token, session and server settings.",
            response.status().as_u16()
        )
        .into());
    }
    if let Some(id) = response.headers().get("mcp-session-id") {
        *session.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.to_str()?.to_string());
    }
    if response.status() == reqwest::StatusCode::ACCEPTED {
        return Ok(());
    }
    let streaming = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.starts_with("text/event-stream"));
    let mut buffer = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        buffer.extend_from_slice(&chunk);
        if buffer.len() > 96 * 1024 * 1024 {
            return Err("MCP response exceeds the bridge limit.".into());
        }
        if streaming {
            while let Some(end) = buffer.windows(2).position(|s| s == b"\n\n") {
                let event: Vec<_> = buffer.drain(..end + 2).collect();
                for line in std::str::from_utf8(&event)?.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        output(data)?;
                    }
                }
            }
        }
    }
    if !streaming && !buffer.is_empty() {
        output(std::str::from_utf8(&buffer)?)?;
    }
    Ok(())
}

fn output(data: &str) -> Result<(), Error> {
    let value: serde_json::Value = serde_json::from_str(data)?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{value}")?;
    stdout.flush()?;
    Ok(())
}
