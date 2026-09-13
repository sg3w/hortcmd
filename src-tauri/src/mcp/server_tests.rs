use super::*;
use serde_json::json;

fn test_config() -> super::super::McpConfig {
    super::super::McpConfig {
        enabled: true,
        roots: vec![std::env::temp_dir().to_string_lossy().into_owned()],
        max_read_bytes: 1024,
        ..Default::default()
    }
}

fn token() -> AppResult<String> {
    Ok("integration-test-token".into())
}

async fn rpc(
    client: &reqwest::Client,
    url: &str,
    session: Option<&str>,
    value: Value,
) -> reqwest::Response {
    let mut request = client
        .post(url)
        .bearer_auth("integration-test-token")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(value.to_string());
    if let Some(session) = session {
        request = request.header("mcp-session-id", session);
    }
    request.send().await.unwrap()
}

async fn payload(response: reqwest::Response) -> Value {
    assert_eq!(response.status(), 200);
    let text = response.text().await.unwrap();
    let body = text
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .next_back()
        .unwrap_or(&text);
    serde_json::from_str(body).unwrap()
}

#[tokio::test]
async fn real_http_handshake_auth_scope_and_bounded_reads() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let (stop, stopped) = oneshot::channel();
    let sessions = Arc::new(Mutex::new(HashMap::new()));
    let server = tokio::spawn(serve(
        listener,
        stopped,
        sessions.clone(),
        Services {
            token,
            generation: || 0,
            config: test_config,
        },
    ));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    assert_eq!(
        client.post(&url).body("{}").send().await.unwrap().status(),
        403
    );
    assert_eq!(
        client
            .post(&url)
            .bearer_auth("wrong")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    for header in ["origin", "host"] {
        assert_eq!(
            client
                .post(&url)
                .bearer_auth("integration-test-token")
                .header(header, "https://foreign.example")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
    }
    let initial = rpc(&client, &url, None, json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "clientInfo": { "name": "test" } } })).await;
    let session = initial.headers()["mcp-session-id"]
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        payload(initial).await["result"]["protocolVersion"],
        "2025-06-18"
    );
    let notification = rpc(
        &client,
        &url,
        Some(&session),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;
    assert_eq!(notification.status(), 202);
    assert!(notification.bytes().await.unwrap().is_empty());
    for method in ["tools/list", "resources/list"] {
        let result = payload(
            rpc(
                &client,
                &url,
                Some(&session),
                json!({ "jsonrpc": "2.0", "id": 2, "method": method }),
            )
            .await,
        )
        .await;
        assert!(result.get("result").is_some());
    }
    let path = std::env::temp_dir().join(format!("hortcmd-mcp-http-{}", std::process::id()));
    std::fs::write(&path, "hello MCP").unwrap();
    let call = |name: &str, arguments: Value| json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": name, "arguments": arguments } });
    for name in ["stat", "read_file"] {
        let result = payload(
            rpc(
                &client,
                &url,
                Some(&session),
                call(name, json!({ "path": path })),
            )
            .await,
        )
        .await;
        assert_eq!(result["result"]["isError"], false, "{result}");
    }
    let listing = payload(
        rpc(
            &client,
            &url,
            Some(&session),
            call("list_dir", json!({ "path": std::env::temp_dir() })),
        )
        .await,
    )
    .await;
    assert_eq!(listing["result"]["isError"], false);
    for (name, args) in [
        ("read_file", json!({ "path": "/outside/scope" })),
        ("write_file", json!({ "path": path, "contents": "no" })),
    ] {
        assert_eq!(
            payload(rpc(&client, &url, Some(&session), call(name, args)).await).await["result"]
                ["isError"],
            true
        );
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello MCP");
    std::fs::write(&path, vec![0; 1025]).unwrap();
    assert_eq!(
        payload(
            rpc(
                &client,
                &url,
                Some(&session),
                call("read_file", json!({ "path": path }))
            )
            .await
        )
        .await["result"]["isError"],
        true
    );
    std::fs::remove_file(path).unwrap();
    let deleted_session = sessions.lock().unwrap().get(&session).unwrap().clone();
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    assert!(deleted_session.register_job("pending".into(), cancelled.clone()));
    assert_eq!(
        client
            .delete(&url)
            .bearer_auth("integration-test-token")
            .header("mcp-session-id", &session)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    assert!(cancelled.load(std::sync::atomic::Ordering::Relaxed));
    assert!(!deleted_session.register_job("late".into(), cancelled));
    assert_eq!(
        rpc(
            &client,
            &url,
            Some(&session),
            json!({ "jsonrpc": "2.0", "id": 9, "method": "ping" })
        )
        .await
        .status(),
        404
    );
    let remaining_session = Session {
        id: "shutdown".into(),
        ..Session::default()
    };
    let stopped_job = Arc::new(std::sync::atomic::AtomicBool::new(false));
    assert!(remaining_session.register_job("pending".into(), stopped_job.clone()));
    sessions
        .lock()
        .unwrap()
        .insert("shutdown".into(), remaining_session.clone());
    stop.send(()).unwrap();
    server.await.unwrap();
    assert!(stopped_job.load(std::sync::atomic::Ordering::Relaxed));
    assert!(sessions.lock().unwrap().is_empty());
    assert!(!remaining_session.register_job("late".into(), stopped_job));
}

/// Optional interoperability check with the official SDK installed outside the
/// application dependency tree. See scripts/mcp-client-check.mjs.
#[tokio::test]
#[ignore]
async fn official_sdk_http_and_stdio() {
    let sdk = std::env::var("MCP_SDK_ROOT")
        .expect("set MCP_SDK_ROOT to an npm prefix containing @modelcontextprotocol/sdk");
    let bridge = std::env::var("MCP_BRIDGE_BIN")
        .expect("set MCP_BRIDGE_BIN to the built hortcmd-mcp binary");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(serve(
        listener,
        stopped,
        Arc::new(Mutex::new(HashMap::new())),
        Services {
            token,
            generation: || 0,
            config: || super::super::McpConfig {
                allow_write: true,
                confirm_writes: false,
                ..test_config()
            },
        },
    ));
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/mcp-client-check.mjs");
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("node")
            .arg(script)
            .env("MCP_SDK_ROOT", sdk)
            .env("MCP_BRIDGE_BIN", bridge)
            .env("HORTCMD_MCP_PORT", port.to_string())
            .env("HORTCMD_MCP_TOKEN", "integration-test-token")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    stop.send(()).unwrap();
    server.await.unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn token_rotation_during_authentication_cannot_recreate_a_session() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let (stop, stopped) = oneshot::channel();
    let sessions = Arc::new(Mutex::new(HashMap::new()));
    let server = tokio::spawn(serve(
        listener,
        stopped,
        sessions.clone(),
        Services {
            token: || {
                // The request read the old credential while rotation completed.
                GENERATION.fetch_add(1, Ordering::SeqCst);
                token()
            },
            generation: || GENERATION.load(Ordering::SeqCst),
            config: test_config,
        },
    ));
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let response = rpc(&client, &url, None, json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{"protocolVersion":"2025-06-18"}})).await;
    assert_eq!(response.status(), 403);
    assert!(sessions.lock().unwrap().is_empty());
    stop.send(()).unwrap();
    server.await.unwrap();
}
