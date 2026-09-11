// ============================================================
// Live provider tests against real servers (tickets 022–025).
//
// These are `#[ignore]`d: they need reachable test servers and are
// therefore not part of the normal `cargo test` run. Start the servers
// (see each test's doc comment) and run explicitly:
//
//   cargo test --lib live_ -- --ignored --nocapture --test-threads=1
//
// The point of these tests is to exercise the *protocol* code — the
// parts a compile check cannot prove: connect/auth, listing, streaming
// read/write, rename and delete.
// ============================================================

use super::Provider;
use crate::net::hostkey::{self, ConnectError, HostId};
use crate::net::session::Profile;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

/// SFTP connect for the live tests. A fresh test server is always an
/// unknown host, so the host-key prompt (ticket 028) is answered here
/// exactly the way the connection manager answers it.
fn sftp_connect(prof: &Profile, secret: &str) -> Arc<dyn Provider> {
    match super::sftp::connect(prof, Some(secret)) {
        Ok(provider) => provider,
        Err(ConnectError::HostKey(challenge)) => {
            let token = challenge
                .token
                .expect("an unknown host must offer a trust token");
            hostkey::trust(&token).expect("persist the accepted host key");
            super::sftp::connect(prof, Some(secret)).expect("sftp connect after trusting")
        }
        Err(e) => panic!("sftp connect: {}", e),
    }
}

fn profile(
    protocol: &str,
    host: &str,
    port: u16,
    user: &str,
    auth: &str,
    start_path: &str,
) -> Profile {
    Profile {
        id: format!("live-{}", protocol),
        name: format!("live {}", protocol),
        protocol: protocol.to_string(),
        host: host.to_string(),
        port,
        user: user.to_string(),
        auth: auth.to_string(),
        key_path: None,
        start_path: start_path.to_string(),
    }
}

/// Runs the full `Provider` surface against a writable directory.
/// `base` must exist and be writable for the connected account.
fn exercise(p: &dyn Provider, base: &str) {
    let dir = format!("{}/hortcmd-live", base.trim_end_matches('/'));
    let file = format!("{}/hello.txt", dir);
    let renamed = format!("{}/renamed.txt", dir);
    let payload = b"hallo aus hortcmd";

    // Clean up a leftover run so the test is repeatable.
    let _ = p.remove_dir_all(Path::new(&dir));

    // --- list (the parent must at least be readable) ---
    let listing = p.list(base).expect("list base");
    println!(
        "  list({}) -> {} entries, path={}",
        base,
        listing.entries.len(),
        listing.path
    );

    // --- mkdir ---
    p.create_dir_all(Path::new(&dir)).expect("create_dir_all");
    assert!(p.is_dir(Path::new(&dir)), "created dir must be a directory");
    assert!(p.exists(Path::new(&dir)), "created dir must exist");

    // --- streaming write ---
    {
        let mut w = p.create_write(Path::new(&file)).expect("create_write");
        w.write_all(payload).expect("write_all");
        // flush() completes remote uploads — errors must surface here.
        w.flush().expect("flush");
    }

    // --- stat ---
    let st = p.stat(Path::new(&file)).expect("stat file");
    assert!(!st.is_dir, "file must not be a dir");
    assert_eq!(st.len, payload.len() as u64, "size mismatch");

    // --- streaming read ---
    let mut got = Vec::new();
    p.open_read(Path::new(&file))
        .expect("open_read")
        .read_to_end(&mut got)
        .expect("read_to_end");
    assert_eq!(got, payload, "round-tripped content differs");

    // --- read_dir sees the file ---
    let kids = p.read_dir(Path::new(&dir)).expect("read_dir");
    let names: Vec<String> = kids
        .iter()
        .map(|k| k.file_name().unwrap_or_default().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"hello.txt".to_string()), "read_dir: {:?}", names);

    // --- rename ---
    p.rename(Path::new(&file), Path::new(&renamed))
        .expect("rename");
    assert!(!p.exists(Path::new(&file)), "old name must be gone");
    assert!(p.exists(Path::new(&renamed)), "new name must exist");

    // --- whole-file write helper ---
    p.write(Path::new(&file), b"zweite datei").expect("write");
    assert_eq!(p.stat(Path::new(&file)).expect("stat 2").len, 12);

    // --- delete ---
    p.remove_file(Path::new(&renamed)).expect("remove_file");
    assert!(!p.exists(Path::new(&renamed)), "removed file must be gone");

    p.remove_dir_all(Path::new(&dir)).expect("remove_dir_all");
    assert!(!p.exists(Path::new(&dir)), "removed dir must be gone");

    println!("  OK: mkdir/write/stat/read/read_dir/rename/delete");
}

/// Exercises the staging/commit contract a failure-safe transfer relies
/// on (ticket 029): a staged entry is published with one rename, and a
/// replacement keeps the old destination readable until the new one is
/// in place. Run against every remote provider, because each has its
/// own rename semantics.
fn exercise_staging(p: &dyn Provider, base: &str) {
    use crate::commands::fs::staging_test_support::commit;

    assert!(
        p.supports_staging(),
        "a provider used for safe transfers must declare staging support"
    );

    let dir = format!("{}/hortcmd-stage", base.trim_end_matches('/'));
    let _ = p.remove_dir_all(Path::new(&dir));
    p.create_dir_all(Path::new(&dir)).expect("create dir");

    let target = format!("{}/report.txt", dir);
    let stage_new = format!("{}/.hortcmd-stage-live-1", dir);
    let stage_replace = format!("{}/.hortcmd-stage-live-2", dir);

    let write = |path: &str, data: &[u8]| {
        let mut w = p.create_write(Path::new(path)).expect("create_write");
        w.write_all(data).expect("write_all");
        w.flush().expect("flush");
    };
    let read = |path: &str| {
        let mut got = Vec::new();
        p.open_read(Path::new(path))
            .expect("open_read")
            .read_to_end(&mut got)
            .expect("read_to_end");
        got
    };

    // --- commit of a new destination ---
    write(&stage_new, b"first");
    commit(p, Path::new(&stage_new), Path::new(&target), false).expect("commit new");
    assert_eq!(read(&target), b"first", "commit must publish the staged bytes");
    assert!(!p.exists(Path::new(&stage_new)), "staging entry must be gone");

    // --- commit over an existing destination ---
    write(&stage_replace, b"second");
    commit(p, Path::new(&stage_replace), Path::new(&target), true).expect("commit replace");
    assert_eq!(read(&target), b"second", "replacement must win");
    assert!(!p.exists(Path::new(&stage_replace)));

    // --- no working entries are left behind ---
    let left: Vec<String> = p
        .read_dir(Path::new(&dir))
        .expect("read_dir")
        .iter()
        .filter_map(|c| c.file_name()?.to_str().map(str::to_string))
        .filter(|n| n.starts_with(".hortcmd-"))
        .collect();
    assert!(left.is_empty(), "leftover working entries: {:?}", left);

    let _ = p.remove_dir_all(Path::new(&dir));
    println!("  OK: staging commit (new + replace), nothing left behind");
}

/// Checks the capability answer for a *live* connection (ticket 031).
///
/// Called after [`exercise`], so every capability asserted here has just
/// been performed against the real server — the matrix reports tested
/// behavior, not what happened to compile. The local-only commands are
/// asserted to be refused with the typed `UNSUPPORTED` marker, so the
/// frontend disabling them is backed by a backend that says no.
fn exercise_capabilities(p: Arc<dyn Provider>, address: &str, ssh_expected: bool) {
    use crate::vfs::caps::{self, Capability, UNSUPPORTED};

    let store_id = p.store_id();
    crate::net::session::register_live(p);

    let caps = caps::for_address(address).expect("capabilities of a live address");
    assert!(caps.connected, "{} should be reported as connected", address);
    assert_eq!(caps.store_id, store_id, "cache identity of {}", address);

    // Everything `exercise` just proved against the real server.
    for cap in [
        Capability::Browse,
        Capability::Preview,
        Capability::CopyFrom,
        Capability::CopyTo,
        Capability::Move,
        Capability::Create,
        Capability::DeletePermanent,
    ] {
        assert!(caps.has(cap), "{} must advertise {}", address, cap.id());
        caps::require(address, cap).expect("advertised capability must pass the gate");
    }

    // A shell only where the connection really carries one.
    assert_eq!(
        caps.has(Capability::SshTerminal),
        ssh_expected,
        "ssh terminal on {}",
        address
    );

    // Local-only command implementations stay refused until their own
    // ticket routes them through the VFS.
    for cap in [
        Capability::Trash,
        Capability::SystemOpen,
        Capability::OpenWith,
        Capability::QuickLook,
        Capability::LocalTerminal,
        Capability::VolumeInfo,
        Capability::GitStatus,
        Capability::FinderTags,
        Capability::Search,
        Capability::RecursiveSize,
        Capability::Checksums,
        Capability::Stat,
        Capability::Permissions,
        Capability::CompareDirs,
        Capability::CompareFiles,
        Capability::ArchiveBrowse,
        Capability::ArchiveExtractFrom,
        Capability::ArchivePackFrom,
    ] {
        assert!(!caps.has(cap), "{} must not advertise {}", address, cap.id());
        let err = caps::require(address, cap)
            .expect_err("an unadvertised capability must be refused");
        assert!(err.starts_with(UNSUPPORTED), "untyped refusal: {}", err);
    }
    println!("  OK: capability matrix matches the exercised behavior");
}

/// SFTP. Server:
/// `docker run -d --name sftp -p 22:22 emberstack/sftp demo:demo123:::upload`
#[test]
#[ignore]
fn live_sftp() {
    let prof = profile("sftp", "127.0.0.1", 22, "demo", "password", "/upload");
    let p = sftp_connect(&prof, "demo123");
    println!("SFTP store_id={}", p.store_id());
    exercise(&*p, "/upload");
    exercise_staging(&*p, "/upload");
    exercise_capabilities(p, "sftp://demo@127.0.0.1:22/upload", true);
}

/// Host-key verification (ticket 028) against a server with a fixed
/// key: first contact prompts, trusting persists, the reconnect is
/// silent, and a rotated key is rejected outright.
///
/// Same server as `live_ssh_shell`:
/// `docker run -d --name hortcmd-ssh -p 2222:2222 -e PASSWORD_ACCESS=true \
///   -e USER_NAME=demo -e USER_PASSWORD=demo123 -e SUDO_ACCESS=false \
///   lscr.io/linuxserver/openssh-server`
#[test]
#[ignore]
fn live_ssh_host_key() {
    let prof = profile("sftp", "127.0.0.1", 2222, "demo", "password", "/config");
    let id = HostId::new(&prof.host, prof.port);
    hostkey::forget(&id).expect("clean slate");

    // 1. First contact. The password is deliberately wrong: reaching the
    //    challenge instead of an auth failure proves the host key is
    //    checked *before* any authentication method runs.
    let challenge = match super::sftp::connect(&prof, Some("definitely-wrong")) {
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
        "  host key {} {} {}:{}",
        challenge.key_type, challenge.fingerprint, challenge.host, challenge.port
    );

    // 2. Trusting persists the key; the retried connection succeeds.
    hostkey::trust(&challenge.token.expect("trust token")).expect("persist host key");
    let p = super::sftp::connect(&prof, Some("demo123")).expect("connect after trusting");
    drop(p);

    // 3. A later connection with the same key must not prompt again.
    super::sftp::connect(&prof, Some("demo123")).expect("silent reconnect");

    // 4. Rotate the stored key: the server now presents something else.
    hostkey::forget(&id).expect("drop the trusted key");
    let rotated = russh::keys::parse_public_key_base64(
        "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ",
    )
    .expect("fixture key");
    hostkey::record(&id, &rotated).expect("stage the rotated key");

    match super::sftp::connect(&prof, Some("demo123")) {
        Err(ConnectError::HostKey(c)) => {
            assert_eq!(c.status, "mismatch");
            assert!(c.token.is_none(), "a mismatch must offer no bypass");
            assert_eq!(c.stored.len(), 1);
            assert_ne!(c.stored[0].fingerprint, c.fingerprint);
            println!("  rejected: stored {} vs {}", c.stored[0].fingerprint, c.fingerprint);
        }
        Ok(_) => panic!("a changed host key must not connect"),
        Err(e) => panic!("expected a mismatch, got: {}", e),
    }

    hostkey::forget(&id).expect("clean up");
}

/// SSH terminal (ticket 026): opens a PTY shell on the SFTP connection,
/// runs a command, reports a new size and closes the session — exactly
/// the path `ssh_open_shell`/`ssh_send`/`ssh_resize`/`ssh_close` take,
/// minus the Tauri event plumbing.
///
/// Needs a server that grants a *shell* (the sftp-only image of
/// `live_sftp` does not):
/// `docker run -d --name hortcmd-ssh -p 2222:2222 -e PASSWORD_ACCESS=true \
///   -e USER_NAME=demo -e USER_PASSWORD=demo123 -e SUDO_ACCESS=false \
///   lscr.io/linuxserver/openssh-server`
#[test]
#[ignore]
fn live_ssh_shell() {
    use crate::net::session::runtime;
    use crate::net::terminal::open_shell_channel;
    use russh::ChannelMsg;

    let prof = profile("sftp", "127.0.0.1", 2222, "demo", "password", "/config");
    let p = sftp_connect(&prof, "demo123");
    // The terminal rides on the connection the host-key check already
    // verified — it never opens a second, unverified one (ticket 028).
    let ssh = p.ssh().expect("sftp provider must carry its SSH session");

    runtime().block_on(async move {
        let channel = open_shell_channel(&ssh, "/config", 80, 24)
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
        println!("  shell output:\n{}", out);
        assert!(
            out.contains("HORTCMD-42"),
            "command output missing in shell session"
        );
        // Closing an already ended session must not error.
        let _ = write.close().await;
    });
    println!("  OK: pty/shell/input/output/resize/close");
}

/// FTP. Server:
/// `docker run -d -p 21:21 -p 21000-21010:21000-21010 -e USERS="one|1234" \
///   -e ADDRESS=localhost delfer/alpine-ftp-server`
/// Note: this image does not chroot the user, so `/` is the container
/// root (not writable by uid 1000) — the account's home is `/ftp/one`.
#[test]
#[ignore]
fn live_ftp() {
    let prof = profile("ftp", "127.0.0.1", 21, "one", "password", "/ftp/one");
    let p = super::ftp::connect(&prof, Some("1234")).expect("ftp connect");
    println!("FTP store_id={}", p.store_id());
    exercise(&*p, "/ftp/one");
    exercise_staging(&*p, "/ftp/one");
    exercise_capabilities(p, "ftp://one@127.0.0.1:21/ftp/one", false);
}

/// SMB. Server:
/// `docker run -d --name samba -p 445:445 -e NAME=Data -e USER=samba \
///   -e PASS=secret -v "$PWD/samba:/storage" dockurr/samba`
#[test]
#[ignore]
fn live_smb() {
    let prof = profile("smb", "127.0.0.1", 445, "samba", "password", "/Data");
    let p = super::smb::connect(&prof, Some("secret")).expect("smb connect");
    println!("SMB store_id={}", p.store_id());

    // Share enumeration (server → shares) goes over the `srvsvc` RPC pipe.
    // Not every server speaks the transfer syntax smb-rs proposes, so this
    // is reported rather than asserted — accessing a share directly works
    // regardless (that is what the file operations below prove).
    match p.list("/") {
        Ok(shares) => {
            let names: Vec<&str> = shares.entries.iter().map(|e| e.name.as_str()).collect();
            println!("  shares: {:?}", names);
        }
        Err(e) => println!("  share enumeration NOT supported by this server: {}", e),
    }

    exercise(&*p, "/Data");
    exercise_staging(&*p, "/Data");
    exercise_capabilities(p, "smb://samba@127.0.0.1:445/Data", false);
}

/// WebDAV. Server:
/// `docker run -d --name dav -p 8080:80 -e USERNAME=dav -e PASSWORD=davpass \
///   bytemark/webdav`
///
/// Note: plain HTTP on a non-80 port needs the scheme spelled out in the
/// host field (otherwise a non-80 port implies HTTPS).
#[test]
#[ignore]
fn live_webdav() {
    let prof = profile("webdav", "http://127.0.0.1", 8080, "dav", "password", "/");
    let p = super::webdav::connect(&prof, Some("davpass")).expect("webdav connect");
    println!("WebDAV store_id={}", p.store_id());
    exercise(&*p, "/");
    exercise_staging(&*p, "/");
    exercise_capabilities(p, "webdav://dav@127.0.0.1:8080/", false);
}
