//! Guard the Serde/TypeScript boundary when updating the binding generator.
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

#[test]
fn omitted_transport_fields_remain_optional_in_typescript() {
    use crate::net::{lifecycle::Phase, session::ConnectionStatus};
    use crate::vfs::caps::LocationCapabilities;
    let status = ConnectionStatus {
        transport: None,
        id: "test".into(),
        key: "test".into(),
        name: "test".into(),
        connected: false,
        address: "sftp://test/".into(),
        phase: Phase::Disconnected,
        attempt: None,
        error: None,
    };
    let caps = LocationCapabilities {
        transport: None,
        scheme: "local".into(),
        store_id: "local".into(),
        connected: true,
        caps: vec![],
    };
    for value in [
        serde_json::to_value(status).unwrap(),
        serde_json::to_value(caps).unwrap(),
    ] {
        assert!(value.get("transport").is_none());
    }
    for declaration in [ConnectionStatus::decl(), LocationCapabilities::decl()] {
        assert!(
            declaration.contains("transport?: TransportInfo"),
            "{declaration}"
        );
    }
}

#[test]
fn generator_understands_omitted_options_without_disabling_serde_guards() {
    #[derive(Serialize, Deserialize, TS)]
    struct OptionalField {
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    }
    assert_eq!(
        serde_json::to_value(OptionalField { value: None }).unwrap(),
        json!({})
    );
    assert_eq!(
        serde_json::to_value(OptionalField {
            value: Some("kept".into())
        })
        .unwrap(),
        json!({"value": "kept"})
    );
    assert_eq!(
        OptionalField::decl(),
        "type OptionalField = { value?: string, };"
    );

    #[derive(Serialize, Deserialize, TS)]
    #[serde(tag = "kind", deny_unknown_fields)]
    enum Guarded {
        Value { name: String },
    }
    assert!(serde_json::from_value::<Guarded>(json!({"kind":"Value", "name":"ok"})).is_ok());
    assert!(
        serde_json::from_value::<Guarded>(json!({"kind":"Value", "name":"ok", "extra":true}))
            .is_err()
    );

    let declaration = Guarded::decl();
    assert!(declaration.contains(r#""kind": "Value""#), "{declaration}");
}
