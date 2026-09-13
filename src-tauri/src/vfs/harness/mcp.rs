//! MCP reads use the very same live provider registration as the GUI.
use super::Harness;
use crate::mcp::{tools, McpConfig};
use serde_json::json;

#[test]
#[ignore]
fn harness_mcp_reads_live_remote_and_refuses_closed_connection() {
    let Some(h) = Harness::load() else {
        eprintln!("SKIP: HORTCMD_HARNESS is not set");
        return;
    };
    for name in h.services.keys() {
        let provider = h.connect(name);
        let store = provider.store_id();
        crate::net::session::register_live(provider);
        let root = h.address(name);
        let config = McpConfig {
            enabled: true,
            roots: vec![root.clone()],
            ..Default::default()
        };
        let list = tools::call_session(
            "harness",
            "harness",
            "list_dir",
            &json!({"path":root}),
            &config,
        );
        assert_eq!(list["isError"], false, "{name}: {list}");
        let file = h
            .service(name)
            .seed
            .iter()
            .find(|entry| !entry.dir && entry.size < 1024)
            .expect("seeded small file");
        let path = format!("{}/{}", root.trim_end_matches('/'), file.path);
        let read = tools::call_session(
            "harness",
            "harness",
            "read_file",
            &json!({"path":path}),
            &config,
        );
        assert_eq!(read["isError"], false, "{name}: {read}");
        let denied = McpConfig {
            roots: vec![format!("{root}/not-this-file")],
            ..config.clone()
        };
        assert_eq!(
            tools::call_session(
                "harness",
                "harness",
                "read_file",
                &json!({"path":path}),
                &denied
            )["isError"],
            true
        );
        crate::net::session::unregister_live(&store);
        assert_eq!(
            tools::call_session(
                "harness",
                "harness",
                "read_file",
                &json!({"path":path}),
                &config
            )["isError"],
            true
        );
    }
}
