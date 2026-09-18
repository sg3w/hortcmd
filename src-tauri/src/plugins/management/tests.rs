use super::*;
use serde_json::json;
fn fixture() -> (
    super::super::package::Staging,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let temp = super::super::package::Staging::new(&std::env::temp_dir()).unwrap();
    let source = temp.path().join("source");
    let root = temp.path().join("plugins");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("index.html"), "hello").unwrap();
    fs::write(source.join("plugin.json"),json!({"id":"org.test.example","name":"Example","description":"Test","author":"Test","version":"1.0.0","apiVersion":1,"runtime":"js","entry":"index.html","icon":"🌈","permissions":["notify"],"contributes":{}}).to_string()).unwrap();
    (temp, source, root)
}
#[test]
fn install_grant_restart_update_and_uninstall() {
    let (_temp, source, root) = fixture();
    let broker = PluginBroker::default();
    let id = install(&broker, &root, &source, None).unwrap();
    let record = records(&broker, &root).unwrap().remove(0);
    assert!(!record.active);
    activate(&broker, &root, &id, record.fingerprint.as_ref().unwrap()).unwrap();
    let restarted = PluginBroker::default();
    assert!(records(&restarted, &root).unwrap()[0].active);
    assert!(install(&broker, &root, &source, Some(&id)).is_err());
    let manifest = source.join("plugin.json");
    let text = fs::read_to_string(&manifest)
        .unwrap()
        .replace("1.0.0", "1.1.0");
    fs::write(manifest, text).unwrap();
    install(&broker, &root, &source, Some(&id)).unwrap();
    assert!(!records(&broker, &root).unwrap()[0].active);
    assert!(activate(&broker, &root, &id, record.fingerprint.as_ref().unwrap()).is_err());
    deactivate(&broker, &root, &id, true).unwrap();
    assert!(records(&broker, &root).unwrap().is_empty());
}
#[test]
fn changed_package_revokes_persisted_grants() {
    let (_temp, source, root) = fixture();
    let broker = PluginBroker::default();
    let id = install(&broker, &root, &source, None).unwrap();
    let hash = records(&broker, &root).unwrap()[0]
        .fingerprint
        .clone()
        .unwrap();
    activate(&broker, &root, &id, &hash).unwrap();
    fs::write(root.join(&id).join("index.html"), "changed").unwrap();
    assert!(!records(&broker, &root).unwrap()[0].active);
    assert!(broker.grants.lock().unwrap().is_empty());
}
#[test]
fn zip_install_rejects_traversal_and_accepts_flat_package() {
    use std::io::Write;
    let (temp, source, root) = fixture();
    let archive = temp.path().join("plugin.zip");
    for name in ["../escape", "index.html"] {
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file(name, options).unwrap();
        zip.write_all(b"hello").unwrap();
        zip.start_file("plugin.json", options).unwrap();
        zip.write_all(&fs::read(source.join("plugin.json")).unwrap())
            .unwrap();
        zip.finish().unwrap();
        let result = install(&PluginBroker::default(), &root, &archive, None);
        if name.starts_with("..") {
            assert!(result.is_err());
            assert!(!temp.path().join("escape").exists());
        } else {
            assert!(result.is_ok());
        }
    }
}
#[cfg(unix)]
#[test]
fn folder_links_are_rejected_without_installing() {
    let (_temp, source, root) = fixture();
    std::os::unix::fs::symlink("index.html", source.join("link")).unwrap();
    assert!(install(&PluginBroker::default(), &root, &source, None).is_err());
    assert!(registry::discover(&root).unwrap().is_empty());
}
