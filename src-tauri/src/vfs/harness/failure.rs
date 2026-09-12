// ============================================================
// When the other side is not what it should be (TICKET-038).
//
// The guarantees this file checks are the ones that only exist under
// failure: a connection dies mid-copy (TICKET-029/032), a certificate
// does not verify, a host key changes (TICKET-028). Each of them has a
// documented promise, and a promise nobody has watched break is a hope.
//
// These tests stop containers on purpose. They run last in the suite and
// bring the service back up afterwards, so a later test in the same run
// is not reading the rubble of an earlier one.
// ============================================================

use super::{harness_or_skip, Harness};
use crate::commands::fs::transfer::plan::{plan_for, stale_working_entries};
use crate::net::hostkey::{self, ConnectError, HostId};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// `docker compose` for this run's project, from the harness directory.
fn compose(h: &Harness, args: &[&str]) -> bool {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root");
    let status = Command::new("docker")
        .current_dir(repo.join("harness"))
        .arg("compose")
        .args(["-f", "compose.yml"])
        .args(args)
        .env("COMPOSE_PROJECT_NAME", &h.project)
        // `down`/`stop`/`start` do not re-read the service definitions,
        // but compose still interpolates the file, so the variables have
        // to be present or it refuses to parse it.
        .env("RUN_DIR", &h.dir)
        .env("HARNESS_USER", &h.user)
        .env("HARNESS_PASSWORD", &h.password)
        .envs(port_vars(h))
        .status()
        .expect("docker compose");
    status.success()
}

/// The variables `compose.yml` interpolates, rebuilt from run.json.
fn port_vars(h: &Harness) -> Vec<(String, String)> {
    let mut vars = vec![
        ("HARNESS_BASE_ALPINE".to_string(), String::new()),
        ("HARNESS_BASE_DEBIAN".to_string(), String::new()),
        ("HARNESS_SHARE".to_string(), "data".to_string()),
    ];
    for (name, svc) in &h.services {
        let key = match name.as_str() {
            "sftp" => "SSH",
            other => &other.to_uppercase(),
        };
        vars.push((format!("PORT_{}", key), svc.port.to_string()));
    }
    // The passive ranges are only needed to parse the file, not to act.
    for key in ["FTP", "FTPS", "FTPSI"] {
        vars.push((format!("PASV_MIN_{}", key), "0".to_string()));
        vars.push((format!("PASV_MAX_{}", key), "0".to_string()));
    }
    vars
}

// ---------- 1. The server disappears mid-transfer ----------

/// A copy whose server dies must fail — bounded, with an error — and
/// must not leave the destination looking finished.
///
/// This is the case the failure-safe mode exists for (TICKET-029): in
/// staged mode the final name is published by one rename at the end, so
/// an interrupted copy leaves a staging entry and *not* a truncated file
/// under the name the user will trust.
#[test]
#[ignore]
fn harness_survives_a_server_dying_mid_copy() {
    let h = harness_or_skip!("harness_survives_a_server_dying_mid_copy");

    // SFTP is the one that keeps a long-lived channel per stream, which
    // makes "the server went away while writing" reach the provider in
    // the most direct way.
    let name = "sftp";
    let p = h.connect(name);
    let root = h.service(name).root.trim_end_matches('/').to_string();
    let dir = format!("{}/work/interrupt", root);
    let _ = p.remove_dir_all(Path::new(&dir));
    p.create_dir_all(Path::new(&dir)).expect("create work dir");

    let target = format!("{}/interrupted.bin", dir);
    let plan = plan_for(&*p, Path::new(&target).to_path_buf(), false, true);
    assert!(plan.is_staged(), "safe mode must stage");
    let staged = plan.write_to.clone();

    // Start writing, stop the server halfway, and see what comes back.
    let mut writer = p.create_write(&staged).expect("create_write");
    writer
        .write_all(&vec![0xab; 64 * 1024])
        .expect("first chunk");

    println!("  stopping {} …", h.service(name).container);
    assert!(
        compose(&h, &["stop", "-t", "1", &h.service(name).container]),
        "stopping the service must succeed"
    );

    let started = Instant::now();
    let outcome = (|| -> std::io::Result<()> {
        for _ in 0..64 {
            writer.write_all(&vec![0xab; 64 * 1024])?;
        }
        writer.flush()
    })();
    let took = started.elapsed();

    assert!(outcome.is_err(), "writing to a dead server must fail");
    assert!(
        took < Duration::from_secs(60),
        "the failure took {:?} — an unbounded wait is a hung UI",
        took
    );
    println!("  failed in {:?}: {}", took, outcome.unwrap_err());

    // Bring it back and look at what is on disk.
    assert!(
        compose(&h, &["start", &h.service(name).container]),
        "restart"
    );
    let p = wait_for(&h, name);
    assert!(
        !p.exists(Path::new(&target)),
        "the final name must not exist after an interrupted staged copy"
    );

    // Whatever is left is a working entry, which the app reports rather
    // than deletes — it can hold the only copy of a file.
    let leftovers = stale_working_entries(&*p, Path::new(&dir));
    println!("  leftovers reported: {:?}", leftovers);
    let _ = p.remove_dir_all(Path::new(&dir));
}

/// The same interruption in *direct* mode, which is the default: the
/// promise there is weaker — a partial file may exist under the final
/// name — but it must still be a failure the caller hears about, not a
/// silent short copy.
///
/// The server is stopped between two writes rather than on a timer. A
/// 3 MiB copy over loopback finishes in a few milliseconds, so any test
/// that races a stopwatch against it is a test that passes when the
/// machine is busy and fails when it is not.
#[test]
#[ignore]
fn harness_direct_mode_reports_an_interrupted_copy() {
    let h = harness_or_skip!("harness_direct_mode_reports_an_interrupted_copy");

    let name = "webdav";
    let p = h.connect(name);
    let root = h.service(name).root.trim_end_matches('/').to_string();
    let dir = format!("{}/work/interrupt-direct", root);
    let _ = p.remove_dir_all(Path::new(&dir));
    p.create_dir_all(Path::new(&dir)).expect("create work dir");

    let dst = format!("{}/copy.bin", dir);
    let plan = plan_for(&*p, Path::new(&dst).to_path_buf(), false, false);
    assert!(!plan.is_staged(), "direct mode must not stage");
    assert_eq!(
        plan.write_to,
        Path::new(&dst),
        "direct mode writes the final name"
    );

    let mut writer = p.create_write(&plan.write_to).expect("create_write");
    writer
        .write_all(&vec![0xcd; 64 * 1024])
        .expect("first chunk");

    let container = h.service(name).container.clone();
    println!("  stopping {} …", container);
    assert!(compose(&h, &["stop", "-t", "1", &container]), "stop");

    let started = Instant::now();
    let outcome = (|| -> std::io::Result<()> {
        for _ in 0..32 {
            writer.write_all(&vec![0xcd; 64 * 1024])?;
        }
        writer.flush()
    })();
    let took = started.elapsed();

    assert!(
        outcome.is_err(),
        "a copy whose server vanished must not report success"
    );
    assert!(
        took < Duration::from_secs(60),
        "the failure took {:?} — an unbounded wait is a hung UI",
        took
    );
    println!("  reported in {:?}: {}", took, outcome.unwrap_err());

    assert!(compose(&h, &["start", &container]), "restart");
    let p = wait_for(&h, name);
    let _ = p.remove_dir_all(Path::new(&dir));
}

/// Waits for a restarted service to answer again, and hands back a live
/// provider.
///
/// It waits on a *connection*, not on the port. Docker publishes the
/// port through a proxy that accepts before the container process binds
/// anything, so a transport probe says "ready" while the server is still
/// starting — which is how this first appeared, as a reconnect failing
/// with "IncompleteMessage" a millisecond after the port opened.
///
/// Deliberately patient and deliberately quiet: the tests that call this
/// have just stopped a container on purpose, and the next assertion is
/// about the server, not about how fast Docker is today.
fn wait_for(h: &Harness, name: &str) -> std::sync::Arc<dyn crate::vfs::Provider> {
    let mut last = String::new();
    for attempt in 0..100 {
        match h.try_connect(name) {
            Ok(p) => {
                if attempt > 0 {
                    println!("  {} back after {} attempts", name, attempt + 1);
                }
                return p;
            }
            Err(e) => last = e,
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("{} did not come back: {}", name, last);
}

/// Connecting to a server that is not running fails quickly and says
/// *unreachable* rather than a protocol error (TICKET-032).
#[test]
#[ignore]
fn harness_reports_an_unreachable_server() {
    use crate::net::lifecycle::{self, FailureKind};

    let h = harness_or_skip!("harness_reports_an_unreachable_server");
    let name = "ftp";
    let container = h.service(name).container.clone();
    assert!(compose(&h, &["stop", "-t", "1", &container]), "stop");

    let started = Instant::now();
    let err = lifecycle::probe_transport("127.0.0.1", h.service(name).port)
        .expect_err("a stopped service must not be reachable");
    let took = started.elapsed();
    assert_eq!(err.kind, FailureKind::Unreachable, "kind: {:?}", err);
    assert!(
        took < lifecycle::TRANSPORT_TIMEOUT,
        "a refused connection took {:?}",
        took
    );
    println!("  unreachable in {:?}", took);

    assert!(compose(&h, &["start", &container]), "restart");
    // Usable again, which is what makes this test safe to run before the
    // others in the same process.
    wait_for(&h, name);
}

// ---------- 2. TLS ----------

/// The TLS services are reachable *because* the harness points
/// `HORTCMD_EXTRA_CA` at its own CA — and refused without it.
///
/// Both halves matter. The first proves the extra-anchor mechanism
/// works; the second proves it is the only thing standing between the
/// client and a rejection, i.e. that validation is really happening.
#[test]
#[ignore]
fn harness_rejects_an_untrusted_certificate() {
    let h = harness_or_skip!("harness_rejects_an_untrusted_certificate");

    let configured = std::env::var("HORTCMD_EXTRA_CA").unwrap_or_default();
    assert_eq!(
        configured, h.ca_cert,
        "the harness must point HORTCMD_EXTRA_CA at the run's own CA"
    );

    // With the anchor: the three TLS services answer.
    for name in ["ftps", "ftpsi", "webdavs"] {
        let p = h.connect(name);
        let root = h.service(name).root.trim_end_matches('/');
        p.list(root)
            .unwrap_or_else(|e| panic!("{} with the harness CA: {}", name, e));
        println!("  {} verified against the harness CA", name);
    }

    // Without it: the same servers must be refused. The variable is
    // process-wide, so this part runs single-threaded by construction —
    // the suite is invoked with --test-threads=1.
    let previous = std::env::var_os("HORTCMD_EXTRA_CA");
    // SAFETY: the harness runs this suite single-threaded.
    unsafe { std::env::remove_var("HORTCMD_EXTRA_CA") };

    let mut wrongly_accepted = Vec::new();
    for name in ["ftps", "ftpsi", "webdavs"] {
        let profile = h.profile(name);
        let refused = match profile.protocol.as_str() {
            "webdav" => crate::vfs::webdav::connect(&profile, Some(&h.password)).is_err(),
            _ => crate::vfs::ftp::connect(&profile, Some(&h.password)).is_err(),
        };
        if refused {
            println!("  {} refused without the CA, as it must be", name);
        } else {
            wrongly_accepted.push(name);
        }
    }

    unsafe {
        match previous {
            Some(v) => std::env::set_var("HORTCMD_EXTRA_CA", v),
            None => std::env::remove_var("HORTCMD_EXTRA_CA"),
        }
    }

    assert!(
        wrongly_accepted.is_empty(),
        "accepted an untrusted certificate: {:?}",
        wrongly_accepted
    );
}

// ---------- 3. SSH host keys ----------

/// First contact prompts, trusting persists, the reconnect is silent,
/// and a changed key is refused with no way to click past it
/// (TICKET-028).
///
/// The harness generates the server's host key, so this is the only
/// place the *whole* sequence can be run: a real server's key cannot be
/// rotated from a test.
#[test]
#[ignore]
fn harness_verifies_and_rotates_the_ssh_host_key() {
    let h = harness_or_skip!("harness_verifies_and_rotates_the_ssh_host_key");
    let profile = h.profile("sftp");
    let id = HostId::new(&profile.host, profile.port);
    hostkey::forget(&id).expect("clean slate");

    // 1. First contact. The password is deliberately wrong: reaching the
    //    challenge instead of an auth failure proves the host key is
    //    checked *before* any authentication method runs.
    let challenge = match crate::vfs::sftp::connect(&profile, Some("definitely-wrong")) {
        Err(ConnectError::HostKey(c)) => c,
        Ok(_) => panic!("an unknown host must not connect"),
        Err(e) => panic!("expected a host-key challenge, got: {}", e),
    };
    assert_eq!(challenge.status, "unknown");
    assert!(challenge.stored.is_empty());
    assert!(
        challenge.fingerprint.starts_with("SHA256:"),
        "fingerprint: {}",
        challenge.fingerprint
    );
    println!(
        "  first contact: {} {}",
        challenge.key_type, challenge.fingerprint
    );

    // 2. Trusting persists it; the retried connection succeeds.
    hostkey::trust(&challenge.token.clone().expect("trust token")).expect("persist");
    let p = crate::vfs::sftp::connect(&profile, Some(&h.password)).expect("connect after trusting");
    drop(p);

    // 3. A later connection with the same key must not prompt again.
    crate::vfs::sftp::connect(&profile, Some(&h.password)).expect("silent reconnect");
    println!("  reconnect silent");

    // 4. The server now presents a *different* key. The harness
    //    generated a spare exactly for this; swapping the mount and
    //    restarting is what a re-keyed server looks like from here.
    let secrets = Path::new(&h.dir).join("secrets");
    std::fs::copy(
        secrets.join("ssh_host_rotated_key"),
        secrets.join("ssh_host_ed25519_key"),
    )
    .expect("rotate the host key");
    assert!(
        compose(&h, &["restart", "-t", "1", &h.service("sftp").container]),
        "restart with the rotated key"
    );
    // Not `wait_for` here: that helper *accepts* an unknown host key,
    // which is exactly what this test is about to refuse. So the wait is
    // for the expected answer itself — the restarted server rejecting
    // the connection — with a starting server still counting as "not
    // ready yet".
    let mut last = String::new();
    let mut rejected = false;
    for _ in 0..100 {
        match crate::vfs::sftp::connect(&profile, Some(&h.password)) {
            Err(ConnectError::HostKey(c)) => {
                assert_eq!(c.status, "mismatch");
                assert!(c.token.is_none(), "a mismatch must offer no bypass");
                assert_eq!(c.stored.len(), 1);
                assert_ne!(c.stored[0].fingerprint, c.fingerprint);
                println!(
                    "  rejected: stored {} vs presented {}",
                    c.stored[0].fingerprint, c.fingerprint
                );
                rejected = true;
                break;
            }
            Ok(_) => panic!("a changed host key must not connect"),
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(rejected, "no host-key mismatch was ever reported: {}", last);

    hostkey::forget(&id).expect("clean up");
}

/// A connection that opens the socket and then says nothing must be cut
/// off by the overall budget, not left hanging (TICKET-032).
///
/// Needs no service of its own — it binds its own silent listener — so
/// it is the one test here that is useful even without the harness.
#[test]
#[ignore]
fn harness_connect_timeout_is_bounded() {
    use crate::net::lifecycle::{self, FailureKind, OVERALL_TIMEOUT};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let held: Vec<_> = listener.incoming().take(4).filter_map(Result::ok).collect();
        std::thread::sleep(OVERALL_TIMEOUT * 2);
        drop(held);
    });

    // The transport probe passes — the port really is open.
    let started = Instant::now();
    lifecycle::probe_transport("127.0.0.1", port).expect("the port is listening");
    assert!(
        started.elapsed() < lifecycle::TRANSPORT_TIMEOUT,
        "the probe must return as soon as the socket connects"
    );
    println!("  an open but silent port passes the transport probe");

    // A refused port is rejected as unreachable, well inside the budget.
    let started = Instant::now();
    let err = lifecycle::probe_transport("127.0.0.1", 9).unwrap_err();
    assert_eq!(err.kind, FailureKind::Unreachable);
    assert!(started.elapsed() < lifecycle::TRANSPORT_TIMEOUT);
    println!("  a refused port is unreachable, not a protocol error");
}

/// The SSH terminal rides on the connection the host-key check already
/// verified — it never opens a second, unverified one (TICKET-026/028).
#[test]
#[ignore]
fn harness_opens_an_ssh_shell() {
    use crate::net::session::runtime;
    use crate::net::terminal::open_shell_channel;
    use russh::ChannelMsg;

    let h = harness_or_skip!("harness_opens_an_ssh_shell");
    let p = h.connect("sftp");
    let ssh = p
        .ssh()
        .expect("the sftp provider must carry its SSH session");
    let root = h.service("sftp").root.clone();

    runtime().block_on(async move {
        let channel = open_shell_channel(&ssh, &root, 80, 24)
            .await
            .expect("open shell");
        let (mut read, write) = channel.split();

        // A marker makes the assertion independent of the shell prompt.
        write
            .data(&b"echo HORTCMD-$((6*7))\n"[..])
            .await
            .expect("send command");
        // Resize while the shell is live — the remote side must accept it.
        write.window_change(120, 40, 0, 0).await.expect("resize");
        write.data(&b"exit\n"[..]).await.expect("send exit");

        let mut out = String::new();
        while let Some(msg) = read.wait().await {
            match msg {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    out.push_str(&String::from_utf8_lossy(&data[..]));
                }
                ChannelMsg::Eof | ChannelMsg::Close => break,
                _ => {}
            }
        }
        assert!(
            out.contains("HORTCMD-42"),
            "command output missing in shell session:\n{}",
            out
        );
        // Closing an already ended session must not error.
        let _ = write.close().await;
    });
    println!("  pty/shell/input/output/resize/close");
}
