// ============================================================
// One client, one conversation (TICKET-041).
//
// Dispatch: a parsed JSON-RPC request in, a response out — or nothing,
// for a notification. Everything stateful about a client lives here,
// which is very little: its name, and whether it has finished the
// handshake.
//
// The **resources** are the reason this interface is worth more than a
// generic filesystem server. `hortcmd://panes/active` is the folder and
// the selection the user is looking at *right now*, so "compress what I
// have selected" works without anyone typing a path. They are read-only
// and they are published by the window; nothing here can move a pane,
// focus the application or open anything. The interface observes the
// user interface, it does not drive it.
// ============================================================

use super::{panes, protocol, tools, McpConfig};
use protocol::{failure, initialize_result, rpc_error, success, Request};
use serde_json::{json, Value};

/// What one connected client is, as far as the server is concerned.
#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub jobs: std::sync::Arc<
        std::sync::Mutex<
            std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>,
        >,
    >,
    /// Self-reported at `initialize`. A label for the audit log and the
    /// confirmation dialog — never a permission.
    pub client: String,
    /// Whether `initialize` has happened.
    pub ready: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            id: String::new(),
            closed: Default::default(),
            jobs: Default::default(),
            client: "unknown".to_string(),
            ready: false,
        }
    }
}

impl Session {
    /// Serialize closing and job registration so a previously cloned session
    /// cannot start work after its server-side identity has been revoked.
    pub fn register_job(
        &self,
        id: String,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> bool {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        if self.closed.load(std::sync::atomic::Ordering::Relaxed)
            || jobs.len() >= 8
            || jobs.contains_key(&id)
        {
            return false;
        }
        jobs.insert(id, cancel);
        true
    }

    pub fn close(&self) {
        let jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        self.closed
            .store(true, std::sync::atomic::Ordering::Relaxed);
        for cancel in jobs.values() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        super::confirm::forget_session(&self.id);
    }
}

/// Answers one request. `None` for a notification.
pub fn handle(session: &mut Session, request: Request, config: &McpConfig) -> Option<Value> {
    let id = request.id.clone();
    let is_notification = id.is_none();
    if is_notification && !request.method.starts_with("notifications/") {
        return None;
    }
    if !session.ready && !matches!(request.method.as_str(), "initialize" | "ping") {
        return id.map(|id| failure(Some(id), rpc_error::INVALID_REQUEST, "initialize first"));
    }

    let result: Result<Value, (i64, String)> = match request.method.as_str() {
        "initialize" => {
            // The client's name comes from its own `clientInfo`. Taken
            // as given, because there is nothing to verify it against —
            // and used only where a label is wanted.
            if let Some(name) = request
                .params
                .get("clientInfo")
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str)
            {
                session.client = name.to_string();
            }
            session.ready = true;
            Ok(initialize_result(
                request
                    .params
                    .get("protocolVersion")
                    .and_then(Value::as_str),
            ))
        }

        // The client telling us it is ready. Nothing to do and nothing
        // to answer — it is a notification.
        "notifications/initialized" => Ok(Value::Null),
        "notifications/cancelled" => {
            if let Some(id) = request.params.get("requestId") {
                if let Some(cancel) = session
                    .jobs
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&id.to_string())
                {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Ok(Value::Null)
        }

        "ping" => Ok(json!({})),

        "tools/list" => Ok(tools::listing(config)),

        "tools/call" => {
            let name = request.params.get("name").and_then(Value::as_str);
            match name {
                Some(name) => {
                    let arguments = request
                        .params
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    Ok(tools::call_session(
                        &session.id,
                        &session.client,
                        name,
                        &arguments,
                        config,
                    ))
                }
                None => Err((rpc_error::INVALID_PARAMS, "no tool name".to_string())),
            }
        }

        "resources/list" => Ok(if config.allow_read {
            resource_listing()
        } else {
            json!({"resources":[]})
        }),

        "resources/read" => match request.params.get("uri").and_then(Value::as_str) {
            Some(_) if !config.allow_read => Err((-32001, "read access is disabled".into())),
            Some(uri) if !matches!(uri, PANES_ACTIVE | PANES_LEFT | PANES_RIGHT | CONNECTIONS) => {
                Err((-32002, "resource not found".into()))
            }
            Some(uri) => Ok(read_resource(uri, config)),
            None => Err((rpc_error::INVALID_PARAMS, "no uri".to_string())),
        },

        // Declared in the handshake as absent, so a client asking is
        // asking for something we said we do not have.
        "prompts/list" => Ok(json!({ "prompts": [] })),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),

        other => Err((rpc_error::METHOD_NOT_FOUND, format!("no method {}", other))),
    };

    if is_notification {
        return None;
    }
    let id = id.unwrap_or(Value::Null);
    Some(match result {
        Ok(value) => success(id, value),
        Err((code, message)) => failure(Some(id), code, &message),
    })
}

// ---------- Resources ----------

const PANES_ACTIVE: &str = "hortcmd://panes/active";
const PANES_LEFT: &str = "hortcmd://panes/left";
const PANES_RIGHT: &str = "hortcmd://panes/right";
const CONNECTIONS: &str = "hortcmd://connections";

fn resource_listing() -> Value {
    json!({
        "resources": [
            {
                "uri": PANES_ACTIVE,
                "name": "Active pane",
                "description":
                    "The folder the user is looking at and what is selected in it.",
                "mimeType": "application/json",
            },
            {
                "uri": PANES_LEFT,
                "name": "Left pane",
                "description": "The left pane's folder, selection and view mode.",
                "mimeType": "application/json",
            },
            {
                "uri": PANES_RIGHT,
                "name": "Right pane",
                "description": "The right pane's folder, selection and view mode.",
                "mimeType": "application/json",
            },
            {
                "uri": CONNECTIONS,
                "name": "Open connections",
                "description":
                    "Names and schemes of the remote connections that are open. \
                     No hosts, no users, no credentials.",
                "mimeType": "application/json",
            },
        ]
    })
}

fn read_resource(uri: &str, config: &McpConfig) -> Value {
    let value = match uri {
        PANES_ACTIVE => panes::active(),
        PANES_LEFT => panes::side("left"),
        PANES_RIGHT => panes::side("right"),
        CONNECTIONS => Some(panes::connections()),
        _ => None,
    };

    let Some(mut value) = value else {
        // The window has not published anything yet — it is starting, or
        // the interface is running without one.
        return contents(uri, json!({ "available": false }));
    };

    // A pane the client may not reach is still worth naming — "there is
    // a folder open here and you cannot see it" is more useful than
    // silence, and it is what the user configured.
    if uri != CONNECTIONS {
        if let Some(path) = value.get("path").and_then(Value::as_str) {
            let reachable = super::scope::require_in_scope(&config.roots, path).is_ok();
            if !reachable {
                value = json!({
                    "inScope": false,
                    "note": "outside the folders this interface may reach",
                });
            } else if let Some(obj) = value.as_object_mut() {
                obj.insert("inScope".into(), Value::Bool(true));
                let path = obj
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if let Some(selection) = obj.get_mut("selection").and_then(Value::as_array_mut) {
                    selection.retain(|entry| {
                        entry.as_str().is_some_and(|name| {
                            !name.is_empty()
                                && !matches!(name, "." | "..")
                                && !name.contains(['/', '\\'])
                                && super::scope::require_in_scope(
                                    &config.roots,
                                    &format!("{}/{}", path.trim_end_matches('/'), name),
                                )
                                .is_ok()
                        })
                    });
                }
            }
        }
    }
    contents(uri, value)
}

fn contents(uri: &str, value: Value) -> Value {
    json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // These two integration-style resource tests share the production snapshot.
    // Hold this lock for the entire clear/publish/read sequence (TICKET-069).
    static PANE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn closing_cancels_registered_jobs_and_refuses_stale_session_clones() {
        use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};
        let session = Session::default();
        let stale = session.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        assert!(session.register_job("1".into(), cancel.clone()));
        session.close();
        assert!(cancel.load(Ordering::Relaxed));
        assert!(!stale.register_job("2".into(), Arc::new(AtomicBool::new(false))));
    }

    fn request(method: &str, params: Value) -> Request {
        Request::parse(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        }))
        .unwrap()
    }

    fn notification(method: &str) -> Request {
        Request::parse(&json!({ "jsonrpc": "2.0", "method": method })).unwrap()
    }

    fn config() -> McpConfig {
        McpConfig {
            roots: vec!["/tmp".into()],
            ..McpConfig::default()
        }
    }

    #[test]
    fn the_handshake_records_the_client_name() {
        let mut s = Session {
            ready: true,
            ..Session::default()
        };
        let out = handle(
            &mut s,
            request(
                "initialize",
                json!({ "clientInfo": { "name": "claude-desktop" } }),
            ),
            &config(),
        )
        .unwrap();

        assert_eq!(s.client, "claude-desktop");
        assert!(s.ready);
        assert_eq!(out["result"]["serverInfo"]["name"], "hortcmd");
    }

    /// A client that sends no name is still a client; it is just not a
    /// named one. Nothing about it may depend on the name.
    #[test]
    fn a_client_without_a_name_is_unknown_rather_than_refused() {
        let mut s = Session {
            ready: true,
            ..Session::default()
        };
        handle(&mut s, request("initialize", json!({})), &config()).unwrap();
        assert_eq!(s.client, "unknown");
        assert!(s.ready);
    }

    #[test]
    fn a_notification_is_never_answered() {
        let mut s = Session {
            ready: true,
            ..Session::default()
        };
        assert!(handle(&mut s, notification("notifications/initialized"), &config()).is_none());
        // Even an unknown method: a notification gets no reply, ever.
        assert!(handle(&mut s, notification("who/knows"), &config()).is_none());
    }

    #[test]
    fn an_unknown_method_is_a_jsonrpc_error() {
        let mut s = Session {
            ready: true,
            ..Session::default()
        };
        let out = handle(&mut s, request("nope", json!({})), &config()).unwrap();
        assert_eq!(out["error"]["code"], rpc_error::METHOD_NOT_FOUND);
        assert_eq!(out["id"], 1);
    }

    #[test]
    fn ping_answers_and_costs_nothing() {
        let mut s = Session {
            ready: true,
            ..Session::default()
        };
        let out = handle(&mut s, request("ping", json!({})), &config()).unwrap();
        assert_eq!(out["result"], json!({}));
    }

    #[test]
    fn calling_a_tool_without_naming_one_is_an_invalid_params_error() {
        let mut s = Session {
            ready: true,
            ..Session::default()
        };
        let out = handle(&mut s, request("tools/call", json!({})), &config()).unwrap();
        assert_eq!(out["error"]["code"], rpc_error::INVALID_PARAMS);
    }

    #[test]
    fn the_resource_list_names_the_panes_and_the_connections() {
        let mut s = Session {
            ready: true,
            ..Session::default()
        };
        let out = handle(&mut s, request("resources/list", json!({})), &config()).unwrap();
        let uris: Vec<&str> = out["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["uri"].as_str().unwrap())
            .collect();
        assert!(uris.contains(&PANES_ACTIVE));
        assert!(uris.contains(&CONNECTIONS));
    }

    #[test]
    fn an_unknown_resource_is_an_error_the_model_can_read() {
        let mut s = Session {
            ready: true,
            ..Session::default()
        };
        let out = handle(
            &mut s,
            request("resources/read", json!({ "uri": "hortcmd://nope" })),
            &config(),
        )
        .unwrap();
        assert_eq!(out["error"]["code"], -32002);
    }

    /// With no window attached the panes are simply not available, which
    /// is a fact to report rather than an error to raise.
    #[test]
    fn a_pane_nobody_has_published_reads_as_unavailable() {
        let _guard = PANE_TEST.lock().unwrap_or_else(|e| e.into_inner());
        panes::clear();
        let mut s = Session {
            ready: true,
            ..Session::default()
        };
        let out = handle(
            &mut s,
            request("resources/read", json!({ "uri": PANES_ACTIVE })),
            &config(),
        )
        .unwrap();
        let text = out["result"]["contents"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"available\": false"), "{}", text);
    }

    /// A pane outside the roots is named but not described — the user
    /// configured that, and pretending the pane does not exist would be
    /// a different lie.
    #[test]
    fn a_pane_outside_the_roots_is_named_without_its_contents() {
        let _guard = PANE_TEST.lock().unwrap_or_else(|e| e.into_inner());
        panes::publish(json!({
            "active": "left",
            "left": { "path": "/etc", "selection": ["passwd"] },
            "right": { "path": "/tmp", "selection": [] },
        }));

        let mut s = Session {
            ready: true,
            ..Session::default()
        };
        let out = handle(
            &mut s,
            request("resources/read", json!({ "uri": PANES_LEFT })),
            &config(),
        )
        .unwrap();
        let text = out["result"]["contents"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"inScope\": false"), "{}", text);
        assert!(
            !text.contains("passwd"),
            "the selection must not leak: {}",
            text
        );

        // …and the pane that *is* inside comes back whole.
        let out = handle(
            &mut s,
            request("resources/read", json!({ "uri": PANES_RIGHT })),
            &config(),
        )
        .unwrap();
        let text = out["result"]["contents"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"inScope\": true"), "{}", text);
        panes::clear();
    }
}
