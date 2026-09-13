// ============================================================
// JSON-RPC 2.0 and the MCP methods (TICKET-041).
//
// The whole wire format, in one file, because it is small: a request is
// `{jsonrpc, id?, method, params?}`, a response is `{jsonrpc, id,
// result | error}`, and a request without an `id` is a notification that
// gets no answer at all.
//
// MCP adds a handshake (`initialize`) and four things worth calling:
// `tools/list`, `tools/call`, `resources/list`, `resources/read`. A tool
// that fails answers with a *result* carrying `isError: true` rather
// than a JSON-RPC error — that is the protocol's rule, and it matters
// here because "denied by scope" is something the model should read and
// reason about, not a transport failure.
// ============================================================

use serde_json::{json, Map, Value};

/// The protocol revision this server implements.
///
/// Answered verbatim when the client asks for it and it is one we know;
/// otherwise ours is offered, which is what the specification says to do
/// and lets a newer client decide whether to continue.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
const KNOWN_VERSIONS: [&str; 2] = ["2025-06-18", "2025-03-26"];

/// JSON-RPC error codes. The first four are the specification's; the
/// last is ours, for a request that arrived without the right token.
pub mod rpc_error {
    pub const PARSE: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
}

/// One parsed request.
#[derive(Debug)]
pub struct Request {
    /// Absent for a notification, which must not be answered.
    pub id: Option<Value>,
    pub method: String,
    pub params: Map<String, Value>,
}

impl Request {
    /// Parses one JSON-RPC request object.
    pub fn parse(value: &Value) -> Result<Request, (Option<Value>, i64, String)> {
        let obj = value.as_object().ok_or((
            None,
            rpc_error::INVALID_REQUEST,
            "not an object".to_string(),
        ))?;
        let id = obj.get("id").cloned().filter(|v| !v.is_null());
        if id
            .as_ref()
            .is_some_and(|v| !v.is_string() && !v.is_i64() && !v.is_u64())
        {
            return Err((
                None,
                rpc_error::INVALID_REQUEST,
                "invalid request id".into(),
            ));
        }

        if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err((
                id,
                rpc_error::INVALID_REQUEST,
                "jsonrpc must be \"2.0\"".to_string(),
            ));
        }
        let method = obj
            .get("method")
            .and_then(Value::as_str)
            .ok_or((
                id.clone(),
                rpc_error::INVALID_REQUEST,
                "no method".to_string(),
            ))?
            .to_string();
        let params = match obj.get("params") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(m)) => m.clone(),
            // Positional parameters are legal JSON-RPC and are not used
            // by MCP; refusing them is clearer than guessing an order.
            Some(_) => {
                return Err((
                    id,
                    rpc_error::INVALID_PARAMS,
                    "params must be an object".to_string(),
                ))
            }
        };
        Ok(Request { id, method, params })
    }
}

pub fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn failure(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message },
    })
}

/// A tool result carrying a structured value *and* its JSON as text.
///
/// Both, deliberately: `structuredContent` is what a client parses, and
/// the text block is what a client that does not yet read it still sees.
pub fn tool_value(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": value,
        "isError": false,
    })
}

/// A failed tool call.
///
/// A *result*, not a JSON-RPC error: the specification reserves those
/// for the transport, and "outside the allowed roots" is something the
/// model should be able to read and act on.
#[cfg(test)]
pub fn tool_error(message: impl Into<String>) -> Value {
    typed_tool_error("mcp.failed", message)
}

pub fn typed_tool_error(code: &str, message: impl Into<String>) -> Value {
    let message = message.into();
    json!({
        "content": [{ "type": "text", "text": message }],
        "structuredContent": {"error": {"code": code, "message": message}},
        "isError": true,
    })
}

/// The answer to `initialize`.
pub fn initialize_result(requested: Option<&str>) -> Value {
    let version = match requested {
        Some(v) if KNOWN_VERSIONS.contains(&v) => v,
        _ => PROTOCOL_VERSION,
    };
    json!({
        "protocolVersion": version,
        "capabilities": {
            // No `listChanged` on either: the tool table is static, and
            // the resources change with the panes but the client is
            // expected to re-read them rather than be pushed at.
            "tools": {},
            "resources": {},
        },
        "serverInfo": {
            "name": "hortcmd",
            "title": "hortcmd file manager",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": concat!(
            "File management through a running hortcmd. Addresses are either local paths ",
            "or VFS addresses of connections the user already has open (sftp://, smb://, ",
            "webdav://, ftp://) — reaching those needs no credentials from you. ",
            "Only paths inside the user's configured roots are reachable; anything else ",
            "is refused, which is not the same as not existing. Write and delete tools ",
            "may be switched off entirely, and may ask the user for confirmation.",
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<Request, (Option<Value>, i64, String)> {
        Request::parse(&serde_json::from_str(raw).unwrap())
    }

    #[test]
    fn a_well_formed_request_is_parsed() {
        let r = parse(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        assert_eq!(r.method, "tools/list");
        assert_eq!(r.id, Some(json!(1)));
        assert!(r.params.is_empty());
    }

    /// A notification has no id and must never be answered — replying to
    /// one is how a client ends up with a response it cannot match.
    #[test]
    fn a_notification_carries_no_id() {
        let r = parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).unwrap();
        assert!(r.id.is_none());
        // An explicit null id is the same thing.
        let r = parse(r#"{"jsonrpc":"2.0","id":null,"method":"x"}"#).unwrap();
        assert!(r.id.is_none());
    }

    #[test]
    fn a_malformed_request_is_refused_with_its_id_where_there_is_one() {
        let (id, code, _) = parse(r#"{"id":7,"method":"x"}"#).unwrap_err();
        assert_eq!(id, Some(json!(7)), "the answer must be matchable");
        assert_eq!(code, rpc_error::INVALID_REQUEST);

        let (_, code, _) = parse(r#"{"jsonrpc":"2.0","id":1}"#).unwrap_err();
        assert_eq!(code, rpc_error::INVALID_REQUEST);

        let (_, code, _) = parse(r#"[1,2,3]"#).unwrap_err();
        assert_eq!(code, rpc_error::INVALID_REQUEST);
    }

    #[test]
    fn positional_params_are_refused_rather_than_guessed() {
        let (_, code, _) =
            parse(r#"{"jsonrpc":"2.0","id":1,"method":"x","params":[1]}"#).unwrap_err();
        assert_eq!(code, rpc_error::INVALID_PARAMS);
    }

    #[test]
    fn the_handshake_echoes_a_version_it_knows_and_offers_its_own_otherwise() {
        let r = initialize_result(Some("2025-03-26"));
        assert_eq!(r["protocolVersion"], "2025-03-26");

        let r = initialize_result(Some("1999-01-01"));
        assert_eq!(r["protocolVersion"], PROTOCOL_VERSION);

        let r = initialize_result(None);
        assert_eq!(r["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(r["serverInfo"]["name"], "hortcmd");
    }

    /// A refusal is a result the model can read, not a transport error.
    #[test]
    fn a_tool_failure_is_a_result_and_says_so() {
        let e = tool_error("outside the allowed roots");
        assert_eq!(e["isError"], true);
        assert_eq!(e["content"][0]["text"], "outside the allowed roots");
        assert!(e.get("error").is_none(), "not a JSON-RPC error");
    }

    #[test]
    fn a_structured_result_carries_text_as_well() {
        let v = tool_value(json!({ "path": "/a", "entries": 2 }));
        assert_eq!(v["structuredContent"]["entries"], 2);
        assert!(v["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("\"entries\""));
        assert_eq!(v["isError"], false);
    }
}
