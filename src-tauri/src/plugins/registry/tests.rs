use super::*;
use serde_json::json;
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "hortcmd-plugin-contract-{}",
            crate::net::unique_id()
        ));
        std::fs::create_dir_all(root.join("org.test.example")).unwrap();
        std::fs::write(root.join("org.test.example/index.html"), "<p>Hello</p>").unwrap();
        Self(root)
    }
    fn write(&self, value: serde_json::Value) {
        std::fs::write(
            self.0.join("org.test.example/plugin.json"),
            value.to_string(),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn manifest() -> serde_json::Value {
    json!({
        "id":"org.test.example", "name":"Example", "description":"Test", "author":"Test", "version":"1.0.0", "apiVersion":1,
        "runtime":"js", "entry":"index.html", "icon":"🌈", "permissions":["ui:preview", "ui:tab"],
        "contributes":{"preview":[{"id":"preview", "title":"Preview", "icon":"🌈", "match":{"kinds":["file"], "extensions":["php"]}}],
        "tabs":[{"id":"tab", "title":"Tab", "icon":"🌈", "surface":"pane", "path":"active"}]}
    })
}
#[test]
fn discovery_and_matching_are_typed_and_do_not_execute() {
    let fixture = Fixture::new();
    fixture.write(manifest());
    let records = discover(&fixture.0).unwrap();
    let m = records[0].manifest.clone().unwrap();
    assert!(m.contributes.tabs[0].closable);
    let mut higher = m.clone();
    higher.id = "org.test.other".into();
    higher.contributes.preview[0].priority = 2;
    let manifests = [m, higher];
    assert_eq!(
        preview_match(&manifests, &PluginViewKind::File, Some("PHP"), None)
            .unwrap()
            .0
            .id,
        "org.test.other"
    );
    assert!(preview_match(&manifests, &PluginViewKind::Folder, None, None).is_none());
    assert!(preview_match(&manifests, &PluginViewKind::File, Some("txt"), None).is_none());
}
#[test]
fn invalid_contracts_report_errors_instead_of_hiding_packages() {
    let fixture = Fixture::new();
    for (pointer, replacement) in [
        ("/apiVersion", json!(2)),
        ("/permissions", json!(["unknown"])),
        ("/permissions", json!([])),
        ("/entry", json!("missing.html")),
        ("/entry", json!("../index.html")),
        ("/contributes/tabs/0/id", json!("preview")),
        ("/contributes/tabs/0/surface", json!("unknown")),
        ("/contributes/preview/0/match/kinds", json!(["unknown"])),
        ("/runtime", json!("native")),
        ("/id", json!("../escape")),
    ] {
        let mut m = manifest();
        *m.pointer_mut(pointer).unwrap() = replacement;
        fixture.write(m);
        let records = discover(&fixture.0).unwrap();
        assert!(records[0].error.is_some(), "{pointer}");
    }
    let mut m = manifest();
    m.as_object_mut().unwrap().remove("name");
    fixture.write(m);
    assert!(discover(&fixture.0).unwrap()[0].error.is_some());
}
#[cfg(unix)]
#[test]
fn symlinks_cannot_supply_executable_entries() {
    let fixture = Fixture::new();
    fixture.write(manifest());
    let entry = fixture.0.join("org.test.example/index.html");
    std::fs::remove_file(&entry).unwrap();
    std::os::unix::fs::symlink("/etc/hosts", &entry).unwrap();
    assert!(discover(&fixture.0).unwrap()[0]
        .error
        .as_ref()
        .unwrap()
        .contains("links"));
}
