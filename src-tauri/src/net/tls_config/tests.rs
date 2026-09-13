use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;
use suppaftp::rustls::pki_types::PrivateKeyDer;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("hortcmd-tls-{}", crate::net::unique_id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn profile() -> Profile {
    Profile {
        security: Default::default(),
        id: crate::net::unique_id(),
        name: "TLS test".into(),
        protocol: "ftps".into(),
        host: "localhost".into(),
        port: 443,
        user: "test".into(),
        auth: "password".into(),
        key_path: None,
        start_path: "/".into(),
    }
}
fn certificate(host: &str, expired: bool, future: bool) -> (rcgen::Certificate, rcgen::KeyPair) {
    let mut params = rcgen::CertificateParams::new(vec![host.into()]).unwrap();
    if expired {
        params.not_before = rcgen::date_time_ymd(2000, 1, 1);
        params.not_after = rcgen::date_time_ymd(2001, 1, 1);
    }
    if future {
        params.not_before = rcgen::date_time_ymd(2090, 1, 1);
        params.not_after = rcgen::date_time_ymd(2091, 1, 1);
    }
    let key = rcgen::KeyPair::generate().unwrap();
    (params.self_signed(&key).unwrap(), key)
}
fn approve(profile: &mut Profile, temp: &Temp, cert: &rcgen::Certificate) {
    let path = temp.0.join("certificate.pem");
    std::fs::write(&path, cert.pem()).unwrap();
    let preview = crate::net::certificate::net_certificate_preview(
        profile.clone(),
        path.to_string_lossy().into_owned(),
    )
    .unwrap();
    profile.security.certificate = Some(preview.trust);
}

/// A real local TLS exchange. The server records application bytes only after
/// the handshake; invalid trust must keep the seeded credential off the server.
fn exchange(
    profile: &Profile,
    cert: &rcgen::Certificate,
    key: &rcgen::KeyPair,
) -> (bool, Vec<u8>, TlsEvidence) {
    let server = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![cert.der().clone()],
        PrivateKeyDer::Pkcs8(key.serialize_der().into()),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let connection = rustls::ServerConnection::new(Arc::new(server)).unwrap();
        let mut stream = rustls::StreamOwned::new(connection, socket);
        let mut bytes = [0; 64];
        match stream.read(&mut bytes) {
            Ok(n) => bytes[..n].to_vec(),
            Err(_) => Vec::new(),
        }
    });
    let (client, evidence) = config(profile).unwrap();
    let socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let connection =
        rustls::ClientConnection::new(Arc::new(client), ServerName::try_from("localhost").unwrap())
            .unwrap();
    let mut stream = rustls::StreamOwned::new(connection, socket);
    let accepted = stream
        .write_all(b"SEEDED_PASSWORD")
        .and_then(|()| stream.flush())
        .is_ok();
    drop(stream);
    (accepted, worker.join().unwrap(), evidence)
}

#[test]
fn approved_pin_connects_but_unknown_and_rotated_certificates_never_receive_credentials() {
    let temp = Temp::new();
    let (cert, key) = certificate("localhost", false, false);
    let mut profile = profile();
    let (accepted, received, _) = exchange(&profile, &cert, &key);
    assert!(!accepted);
    assert!(received.is_empty());
    approve(&mut profile, &temp, &cert);
    let (accepted, received, evidence) = exchange(&profile, &cert, &key);
    assert!(accepted, "{:?}", evidence.failure.lock().unwrap());
    assert_eq!(received, b"SEEDED_PASSWORD");
    let (rotated, rotated_key) = certificate("localhost", false, false);
    let (accepted, received, evidence) = exchange(&profile, &rotated, &rotated_key);
    assert!(!accepted);
    assert!(received.is_empty());
    assert!(evidence
        .failure
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .is(code::NET_CERTIFICATE_CHANGED));
}

#[test]
fn pinning_never_bypasses_expiry_future_validity_or_hostname() {
    for (host, expired, future, expected) in [
        ("localhost", true, false, code::NET_CERTIFICATE_EXPIRED),
        ("localhost", false, true, code::NET_CERTIFICATE_FUTURE),
        (
            "wrong.example",
            false,
            false,
            code::NET_CERTIFICATE_HOSTNAME,
        ),
    ] {
        let temp = Temp::new();
        let (cert, key) = certificate(host, expired, future);
        let mut profile = profile();
        approve(&mut profile, &temp, &cert);
        let (accepted, received, evidence) = exchange(&profile, &cert, &key);
        assert!(!accepted);
        assert!(received.is_empty());
        let failure = evidence.failure.lock().unwrap().clone().unwrap();
        assert!(failure.is(expected), "{expected}: {failure:?}");
    }
}

#[test]
fn approval_is_profile_scoped_and_file_changes_require_new_review() {
    let temp = Temp::new();
    let (cert, _) = certificate("localhost", false, false);
    let mut profile = profile();
    approve(&mut profile, &temp, &cert);
    assert!(crate::net::certificate::approved_der(&profile)
        .unwrap()
        .is_some());
    let mut other = profile.clone();
    other.id = "other".into();
    assert!(config(&other).is_err());
    other = profile.clone();
    other.host = "other.example".into();
    assert!(config(&other).is_err());
    let (rotated, _) = certificate("localhost", false, false);
    std::fs::write(temp.0.join("certificate.pem"), rotated.pem()).unwrap();
    assert!(config(&profile).is_err());
    profile.security.certificate = None;
    assert!(config(&profile).is_ok());
}

#[test]
fn ftps_adapter_never_logs_in_after_tls_rejection_or_certificate_rotation() {
    use std::io::{BufRead, BufReader};
    for mode in ["untrusted", "approved", "rotated", "reject-auth-tls"] {
        let temp = Temp::new();
        let (approved_cert, _) = certificate("localhost", false, false);
        let (server_cert, key) = certificate("localhost", false, false);
        let server = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![server_cert.der().clone()],
            PrivateKeyDer::Pkcs8(key.serialize_der().into()),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut profile = profile();
        profile.port = listener.local_addr().unwrap().port();
        if mode == "approved" {
            approve(&mut profile, &temp, &server_cert);
        }
        if mode == "rotated" {
            approve(&mut profile, &temp, &approved_cert);
        }
        let worker = std::thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut plain = BufReader::new(socket);
            plain
                .get_mut()
                .write_all(b"220 local test server\r\n")
                .unwrap();
            let mut command = String::new();
            plain.read_line(&mut command).unwrap();
            let mut commands = vec![command.trim().to_owned()];
            if mode == "reject-auth-tls" {
                plain
                    .get_mut()
                    .write_all(b"500 TLS unavailable\r\n")
                    .unwrap();
                command.clear();
                if plain.read_line(&mut command).is_ok() && !command.is_empty() {
                    commands.push(command.trim().into());
                }
                return commands;
            }
            plain.get_mut().write_all(b"234 start TLS\r\n").unwrap();
            let connection = rustls::ServerConnection::new(Arc::new(server)).unwrap();
            let mut encrypted =
                BufReader::new(rustls::StreamOwned::new(connection, plain.into_inner()));
            loop {
                command.clear();
                match encrypted.read_line(&mut command) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                commands.push(command.trim().into());
                let response: &[u8] = if command.starts_with("USER") {
                    b"331 password required\r\n"
                } else if command.starts_with("PASS") {
                    b"230 logged in\r\n"
                } else {
                    b"200 OK\r\n"
                };
                if encrypted
                    .get_mut()
                    .write_all(response)
                    .and_then(|()| encrypted.get_mut().flush())
                    .is_err()
                {
                    break;
                }
            }
            commands
        });
        let opened = crate::vfs::ftp::connect(&profile, Some("SEEDED_PASSWORD"));
        if mode == "approved" {
            assert!(opened.is_ok(), "{mode}");
        } else {
            assert!(opened.is_err(), "{mode}");
        }
        if mode == "rotated" {
            assert!(opened
                .as_ref()
                .err()
                .unwrap()
                .is(code::NET_CERTIFICATE_CHANGED));
        }
        drop(opened);
        let commands = worker.join().unwrap();
        assert_eq!(commands[0], "AUTH TLS");
        if mode == "approved" {
            assert!(commands.iter().any(|c| c == "PROT P"));
            assert!(commands.iter().any(|c| c == "PASS SEEDED_PASSWORD"));
        } else {
            assert!(
                !commands
                    .iter()
                    .any(|c| c.starts_with("USER") || c.starts_with("PASS")),
                "{mode}: {commands:?}"
            );
        }
    }
}

#[test]
fn private_ca_approval_is_scoped_and_revocable_without_disabling_peer_checks() {
    let temp = Temp::new();
    let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Local test CA");
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    let ca_key = rcgen::KeyPair::generate().unwrap();
    let ca = ca_params.self_signed(&ca_key).unwrap();
    let mut leaf = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
    leaf.distinguished_name
        .push(rcgen::DnType::CommonName, "localhost");
    let leaf_key = rcgen::KeyPair::generate().unwrap();
    let leaf = leaf.signed_by(&leaf_key, &ca, &ca_key).unwrap();
    let mut profile = profile();
    approve(&mut profile, &temp, &ca);
    assert!(profile.security.certificate.as_ref().unwrap().is_ca);
    let (accepted, received, evidence) = exchange(&profile, &leaf, &leaf_key);
    assert!(accepted, "{:?}", evidence.failure.lock().unwrap());
    assert_eq!(received, b"SEEDED_PASSWORD");
    profile.security.certificate = None;
    let (accepted, received, _) = exchange(&profile, &leaf, &leaf_key);
    assert!(!accepted);
    assert!(received.is_empty());
}

#[test]
fn webdav_adapter_rejects_wrong_hosts_and_never_follows_https_to_http_redirects() {
    for wrong_host in [true, false] {
        let temp = Temp::new();
        let (cert, key) = certificate(
            if wrong_host {
                "wrong.example"
            } else {
                "localhost"
            },
            false,
            false,
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let sink = TcpListener::bind("127.0.0.1:0").unwrap();
        sink.set_nonblocking(true).unwrap();
        let location = format!("http://{}/insecure", sink.local_addr().unwrap());
        let mut profile = profile();
        profile.protocol = "webdav".into();
        profile.port = listener.local_addr().unwrap().port();
        approve(&mut profile, &temp, &cert);
        let server = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            PrivateKeyDer::Pkcs8(key.serialize_der().into()),
        )
        .unwrap();
        let worker = std::thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let connection = rustls::ServerConnection::new(Arc::new(server)).unwrap();
            let mut stream = rustls::StreamOwned::new(connection, socket);
            let mut request = [0; 8192];
            let n = match stream.read(&mut request) {
                Ok(n) => n,
                Err(_) => return Vec::new(),
            };
            let response = format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = stream
                .write_all(response.as_bytes())
                .and_then(|()| stream.flush());
            request[..n].to_vec()
        });
        let opened = crate::vfs::webdav::connect(&profile, Some("SEEDED_PASSWORD"));
        assert!(opened.is_err());
        if wrong_host {
            assert!(opened
                .as_ref()
                .err()
                .unwrap()
                .is(code::NET_CERTIFICATE_HOSTNAME));
        }
        let received = worker.join().unwrap();
        if wrong_host {
            assert!(received.is_empty());
        } else {
            assert!(!received.is_empty());
        }
        assert!(
            matches!(sink.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}

#[test]
fn certificate_preview_shows_readable_ip_hostnames() {
    let temp = Temp::new();
    let params = rcgen::CertificateParams::new(vec!["127.0.0.1".into(), "::1".into()]).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    let path = temp.0.join("ip.pem");
    std::fs::write(&path, cert.pem()).unwrap();
    let preview = crate::net::certificate::net_certificate_preview(
        profile(),
        path.to_string_lossy().into_owned(),
    )
    .unwrap();
    assert_eq!(preview.hostnames, ["127.0.0.1", "::1"]);
}
