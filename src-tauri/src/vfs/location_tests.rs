use super::location::*;
use serde::Deserialize;
#[derive(Deserialize)]
struct Corpus {
    valid: Vec<Row>,
    invalid: Vec<String>,
    legacy: Vec<Legacy>,
}
#[derive(Deserialize)]
struct Row {
    raw: String,
    uri: String,
    store: String,
    path: String,
    parent: Option<String>,
}
#[derive(Deserialize)]
struct Legacy {
    raw: String,
    uri: String,
}
#[test]
fn shared_conformance_corpus() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("../../../fixtures/vfs-locations.json")).unwrap();
    for row in corpus.valid {
        let view = vfs_location(row.raw.clone(), false, None).unwrap();
        assert_eq!(view.uri, row.uri, "{}", row.raw);
        assert_eq!(view.store.0, row.store);
        assert_eq!(view.identity.0, format!("v1:{}\0{}", row.store, row.path));
        assert_eq!(view.parent, row.parent);
        assert_eq!(VfsLocation::parse(&view.uri).unwrap(), view.location);
        let json = serde_json::to_string(&view.location).unwrap();
        assert_eq!(
            serde_json::from_str::<VfsLocation>(&json).unwrap(),
            view.location
        );
    }
    for raw in corpus.invalid {
        assert!(VfsLocation::parse(&raw).is_err(), "{raw}");
    }
    for row in corpus.legacy {
        assert_eq!(migrate_legacy(&row.raw).unwrap().uri(), row.uri);
    }
}
#[test]
fn component_roundtrips_and_containment() {
    let root = ProviderPath::from_raw("/safe").unwrap();
    for name in ["%20", "a?b#c", "one\\two", "é", "e\u{301}", "a@b", "%2e%2e"] {
        let joined = root.join(name).unwrap();
        assert_eq!(ProviderPath::parse(&joined.uri()).unwrap(), joined);
        assert_eq!(joined.parent(), root);
        assert!(root.contains(&joined));
    }
    assert!(!root.contains(&ProviderPath::from_raw("/safe-other/x").unwrap()));
    assert_ne!(root.join("é").unwrap(), root.join("e\u{301}").unwrap());
    assert_ne!(root.join("Case").unwrap(), root.join("case").unwrap());
    for c in 0..=255u32 {
        let ch = char::from_u32(c).unwrap();
        let raw = format!("sftp://h/x{ch}");
        if let Ok(location) = VfsLocation::parse(&raw) {
            assert_eq!(VfsLocation::parse(&location.uri()).unwrap(), location);
        }
    }
}
#[test]
fn serialized_input_is_validated_and_future_versions_refused() {
    let mut value = serde_json::to_value(VfsLocation::parse("sftp://h/safe").unwrap()).unwrap();
    value["version"] = 2.into();
    assert!(serde_json::from_value::<VfsLocation>(value.clone()).is_err());
    value["version"] = 1.into();
    value["location"]["path"]["components"] = serde_json::json!([".."]);
    assert!(serde_json::from_value::<VfsLocation>(value).is_err());
}

#[test]
fn empty_segments_cannot_widen_a_provider_root() {
    let root = ProviderPath::from_raw("/safe//").unwrap();
    assert!(!root.contains(&ProviderPath::from_raw("/safe/x").unwrap()));
    assert!(root.contains(&ProviderPath::from_raw("/safe//x").unwrap()));
}

#[test]
fn equality_and_hash_use_identity_not_display_spelling() {
    let a = VfsLocation::parse("sftp://u@HOST/a%20b").unwrap();
    let b = VfsLocation::parse("sftp://u@host:22/a b").unwrap();
    assert_ne!(a.uri(), b.uri());
    assert_eq!(a, b);
    assert_eq!(std::collections::HashSet::from([a, b]).len(), 1);
}
