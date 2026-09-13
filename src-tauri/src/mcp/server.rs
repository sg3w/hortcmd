//! Loopback-only Streamable HTTP transport. Each tool runs on a blocking
//! worker; a pending approval never holds the session map or the UI thread.
use super::{audit, auth, panes, protocol, session::Session};
use crate::errors::AppResult;
use http_body_util::{BodyExt, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;

type Reply = Response<ResponseBody>;

pub enum ResponseBody {
    Full(Option<Bytes>),
    Stream(tokio::sync::mpsc::Receiver<Value>),
}

impl hyper::body::Body for ResponseBody {
    type Data = Bytes;
    type Error = Infallible;
    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Infallible>>> {
        match self.get_mut() {
            Self::Full(bytes) => {
                std::task::Poll::Ready(bytes.take().map(|b| Ok(hyper::body::Frame::data(b))))
            }
            Self::Stream(rx) => rx.poll_recv(cx).map(|value| {
                value.map(|v| {
                    Ok(hyper::body::Frame::data(Bytes::from(format!(
                        "event: message\ndata: {v}\n\n"
                    ))))
                })
            }),
        }
    }
}
type Sessions = Arc<Mutex<HashMap<String, Session>>>;

#[derive(Clone, Copy)]
struct Services {
    token: fn() -> AppResult<String>,
    generation: fn() -> u64,
    config: fn() -> super::McpConfig,
}

const SERVICES: Services = Services {
    token: auth::token,
    generation: auth::generation,
    config: panes::effective_config,
};

struct Listener {
    port: u16,
    stop: oneshot::Sender<()>,
    sessions: Sessions,
}

fn state() -> &'static Mutex<Option<Listener>> {
    static STATE: OnceLock<Mutex<Option<Listener>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

pub fn status() -> (Option<u16>, usize) {
    let state = state().lock().unwrap_or_else(|e| e.into_inner());
    state.as_ref().map_or((None, 0), |s| {
        (
            Some(s.port),
            s.sessions.lock().unwrap_or_else(|e| e.into_inner()).len(),
        )
    })
}

/// Bind before changing the running listener, so a port conflict leaves the
/// previous server intact. No runtime or thread is started when disabled.
pub fn configure(config: &super::McpConfig) -> AppResult<()> {
    let mut state = state().lock().unwrap_or_else(|e| e.into_inner());
    if config.enabled && state.as_ref().is_some_and(|s| config.port == s.port) {
        return Ok(());
    }
    let next = if config.enabled {
        auth::token()?;
        let socket = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, config.port))?;
        socket.set_nonblocking(true)?;
        let port = socket.local_addr()?.port();
        let (stop, stopped) = oneshot::channel();
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let session_copy = sessions.clone();
        crate::net::session::runtime().spawn(async move {
            if let Ok(listener) = tokio::net::TcpListener::from_std(socket) {
                serve(listener, stopped, session_copy, SERVICES).await;
            }
        });
        Some(Listener {
            port,
            stop,
            sessions,
        })
    } else {
        None
    };
    if let Some(old) = state.take() {
        close_sessions(&old.sessions);
        let _ = old.stop.send(());
    }
    super::confirm::forget_remembered();
    *state = next;
    Ok(())
}

pub fn revoke_sessions() {
    if let Some(listener) = state().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        close_sessions(&listener.sessions);
    }
    super::confirm::forget_remembered();
}

fn close_sessions(sessions: &Sessions) {
    for (_, session) in sessions.lock().unwrap_or_else(|e| e.into_inner()).drain() {
        session.close();
    }
}

async fn serve(
    listener: tokio::net::TcpListener,
    mut stopped: oneshot::Receiver<()>,
    sessions: Sessions,
    services: Services,
) {
    let Ok(local_address) = listener.local_addr() else {
        return;
    };
    let bound_port = local_address.port();
    let permits = Arc::new(tokio::sync::Semaphore::new(32));
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            Some(_) = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { break };
                let Ok(permit) = permits.clone().try_acquire_owned() else { continue };
                let sessions = sessions.clone();
                connections.spawn(async move {
                    let _permit = permit;
                    let service = hyper::service::service_fn(move |r| route(r, sessions.clone(), services, bound_port));
                    let _ = hyper::server::conn::http1::Builder::new()
                        .timer(hyper_util::rt::TokioTimer::new()).header_read_timeout(Duration::from_secs(10))
                        .keep_alive(false).max_buf_size(32 * 1024)
                        .serve_connection(TokioIo::new(stream), service).await;
                });
            }
        }
    }
    connections.abort_all();
    close_sessions(&sessions);
}

fn reply(status: u16, body: impl Into<Bytes>) -> Reply {
    let mut response = Response::new(ResponseBody::Full(Some(body.into())));
    *response.status_mut() =
        hyper::StatusCode::from_u16(status).unwrap_or(hyper::StatusCode::INTERNAL_SERVER_ERROR);
    response.headers_mut().insert(
        "content-type",
        hyper::header::HeaderValue::from_static("application/json"),
    );
    response
}

fn header<'a>(r: &'a Request<Incoming>, key: &str) -> Option<&'a str> {
    r.headers().get(key)?.to_str().ok()
}

async fn route(
    r: Request<Incoming>,
    sessions: Sessions,
    services: Services,
    bound_port: u16,
) -> Result<Reply, Infallible> {
    Ok(route_inner(r, sessions, services, bound_port).await)
}

async fn route_inner(
    r: Request<Incoming>,
    sessions: Sessions,
    services: Services,
    bound_port: u16,
) -> Reply {
    for key in [
        "host",
        "origin",
        "authorization",
        "mcp-session-id",
        "mcp-protocol-version",
    ] {
        if r.headers().get_all(key).iter().count() > 1
            || r.headers().get(key).is_some_and(|v| v.to_str().is_err())
        {
            return reply(400, "");
        }
    }
    if header(&r, "host").and_then(auth::loopback_port) != Some(bound_port) {
        return reply(403, "");
    }
    // Keychain access is blocking and token rotation takes effect per request.
    let generation = (services.generation)();
    let expected = match tokio::task::spawn_blocking(services.token).await {
        Ok(Ok(token)) => token,
        _ => return reply(503, ""),
    };
    if let auth::Admission::Refused(reason) = auth::admit(
        header(&r, "authorization"),
        header(&r, "host"),
        header(&r, "origin"),
        &expected,
    ) {
        audit::record(
            "unknown",
            "http",
            None,
            audit::McpOutcome::Denied,
            Some(auth::refusal(reason).code.to_string()),
        );
        return reply(403, "");
    }
    if r.uri().path() != "/mcp" {
        return reply(404, "");
    }
    let session_id = header(&r, "mcp-session-id").map(str::to_owned);
    if r.method() == hyper::Method::DELETE {
        if let Some(id) = session_id {
            let mut sessions = sessions.lock().unwrap_or_else(|e| e.into_inner());
            if generation != (services.generation)() {
                return reply(403, "");
            }
            let removed = sessions.remove(&id);
            if let Some(session) = removed {
                session.close();
                return reply(204, "");
            }
            return reply(404, "");
        }
        return reply(400, "");
    }
    if r.method() != hyper::Method::POST {
        return reply(405, "");
    }
    if header(&r, "mcp-protocol-version").is_some_and(|v| !matches!(v, "2025-06-18" | "2025-03-26"))
    {
        return reply(400, "");
    }
    let bytes = match tokio::time::timeout(
        Duration::from_secs(10),
        Limited::new(r.into_body(), 1024 * 1024).collect(),
    )
    .await
    {
        Ok(Ok(bytes)) => bytes.to_bytes(),
        _ => return reply(413, ""),
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => {
            return reply(
                400,
                protocol::failure(None, protocol::rpc_error::PARSE, "invalid JSON").to_string(),
            )
        }
    };
    let request = match protocol::Request::parse(&value) {
        Ok(r) => r,
        Err((id, code, message)) => {
            return reply(400, protocol::failure(id, code, &message).to_string())
        }
    };
    let initialize = request.method == "initialize";
    let (id, mut session) = {
        let mut sessions = sessions.lock().unwrap_or_else(|e| e.into_inner());
        // Rotation advances the generation before draining this same map.
        // An old authenticated request can neither recreate nor acquire a session.
        if generation != (services.generation)() {
            return reply(403, "");
        }
        if initialize {
            if sessions.len() >= 32 {
                return reply(503, "");
            }
            if request.id.is_none() {
                return reply(400, "");
            }
            let Ok(id) = auth::random_hex(24) else {
                return reply(503, "");
            };
            let mut s = Session {
                id: id.clone(),
                ..Session::default()
            };
            let result = super::session::handle(&mut s, request, &(services.config)());
            sessions.insert(id.clone(), s);
            let mut response = reply(200, result.unwrap_or(Value::Null).to_string());
            if let Ok(value) = hyper::header::HeaderValue::from_str(&id) {
                response.headers_mut().insert("mcp-session-id", value);
            }
            return response;
        }
        let Some(id) = session_id else {
            return reply(400, "");
        };
        let Some(s) = sessions.get(&id).cloned() else {
            return reply(404, "");
        };
        (id, s)
    };
    if request.id.is_none() {
        super::session::handle(&mut session, request, &(services.config)());
        return reply(202, "");
    }
    let request_id = request.id.as_ref().unwrap().to_string();
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    if !session.register_job(request_id.clone(), cancel.clone()) {
        return reply(409, "");
    }
    let (output, input) = tokio::sync::mpsc::channel(32);
    let context = super::execution::Execution {
        cancel,
        config: services.config,
        progress_token: request
            .params
            .get("_meta")
            .and_then(|m| m.get("progressToken"))
            .cloned(),
        output: output.clone(),
    };
    tokio::task::spawn_blocking(move || {
        let _session_id = id;
        let response = super::execution::run(context, || {
            super::session::handle(&mut session, request, &(services.config)())
        });
        session
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&request_id);
        if let Some(response) = response {
            let _ = output.blocking_send(response);
        }
    });
    let mut response = Response::new(ResponseBody::Stream(input));
    response.headers_mut().insert(
        "content-type",
        hyper::header::HeaderValue::from_static("text/event-stream"),
    );
    response.headers_mut().insert(
        "cache-control",
        hyper::header::HeaderValue::from_static("no-cache"),
    );
    response
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
