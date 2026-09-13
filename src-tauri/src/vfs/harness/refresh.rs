//! The GUI's shared directory service against every real remote transport.
use super::Harness;
use crate::{commands::fs::listings, net::session, vfs::caps};

#[tokio::test]
#[ignore]
async fn harness_shared_directory_reads_follow_connection_lifecycle() {
    let Some(h) = Harness::load() else {
        eprintln!("SKIP: HORTCMD_HARNESS is not set");
        return;
    };
    for name in h.services.keys() {
        let fixture = h.clone();
        let service = name.clone();
        let provider = tokio::task::spawn_blocking(move || fixture.connect(&service))
            .await
            .unwrap();
        let store = provider.store_id();
        let root = h.address(name);
        session::register_live(provider);
        assert!(caps::for_address(&root).unwrap().connected, "{name}");
        let (left, right) =
            tokio::join!(listings::read(root.clone()), listings::read(root.clone()));
        let left = left.unwrap_or_else(|error| panic!("{name}: {error}"));
        let right = right.unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            serde_json::to_value(&left).unwrap(),
            serde_json::to_value(&right).unwrap(),
            "{name}"
        );
        assert!(!left.entries.is_empty(), "{name}");
        session::unregister_live(&store);
        assert!(!caps::for_address(&root).unwrap().connected, "{name}");
        assert!(
            listings::read(root.clone()).await.is_err(),
            "{name}: read after disconnect"
        );
        let fixture = h.clone();
        let service = name.clone();
        session::register_live(
            tokio::task::spawn_blocking(move || fixture.connect(&service))
                .await
                .unwrap(),
        );
        assert!(caps::for_address(&root).unwrap().connected, "{name}");
        assert!(
            listings::read(root).await.is_ok(),
            "{name}: read after reconnect"
        );
        session::unregister_live(&store);
    }
}
