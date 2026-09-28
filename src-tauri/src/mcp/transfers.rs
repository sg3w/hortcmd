//! MCP adapters to the application's transfer and archive services.
use super::{execution, scoped_provider::ScopedProvider, tools::Args};
use crate::commands::fs::transfer::{
    copy::{transfer_one, CopyTuning},
    events::OpProgress,
    plan::Plan,
    progress::{Prog, ProgressSink},
};
use crate::errors::{code, AppError, AppResult};
use crate::vfs::Provider;
use serde_json::{json, Value};
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Instant;

struct Sink;
impl ProgressSink for Sink {
    fn progress(&self, report: OpProgress) {
        execution::progress(
            report.bytes_done,
            (report.bytes_total > 0).then_some(report.bytes_total),
        );
    }
}

fn progress(op: &str) -> Prog {
    Prog {
        sink: Arc::new(Sink),
        id: String::new(),
        op: op.into(),
        files_total: 0,
        bytes_total: 0,
        files_done: 0,
        bytes_done: 0,
        last: Instant::now(),
        cancel: execution::cancel_flag(),
        pause: Arc::new(AtomicBool::new(false)),
        notes: Default::default(),
    }
}

pub fn copy(args: &Args) -> AppResult<Value> {
    transfer(args, false)
}
pub fn move_entry(args: &Args) -> AppResult<Value> {
    transfer(args, true)
}

fn transfer(args: &Args, moving: bool) -> AppResult<Value> {
    let from = args.str("from")?;
    let to = args.str("to")?;
    let (source, src) = ScopedProvider::resolve(&from, args.config)?;
    let (mut destination, dst) = ScopedProvider::resolve(&to, args.config)?;
    if source.store_id() == destination.store_id()
        && (dst.starts_with(&src) || src.starts_with(&dst))
    {
        return Err(
            AppError::new(code::VALIDATION_BAD_INPUT).detail("source and destination overlap")
        );
    }
    if !destination.supports_staging() {
        return Err(AppError::new(code::CAPABILITY_UNSUPPORTED)
            .detail("destination cannot publish staged transfers"));
    }
    let exists = destination.exists(&dst);
    if exists && !args.bool("overwrite", false) {
        return Err(AppError::from(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "destination exists",
        )));
    }
    let plan = Plan::staged(&destination, dst, exists)?;
    let mut prog = progress(if moving { "move" } else { "copy" });
    transfer_planned(&source, &mut destination, &src, &plan, &mut prog, moving)?;
    Ok(
        json!({ "from": from, "to": to, "files": prog.files_done, "bytes": prog.bytes_done, "moved": moving }),
    )
}

fn transfer_planned(
    source: &ScopedProvider,
    destination: &mut ScopedProvider,
    src: &crate::vfs::StreamPath,
    plan: &Plan,
    prog: &mut Prog,
    moving: bool,
) -> AppResult<()> {
    destination.track_stage(&plan.write_to)?;
    let mut errors = Vec::new();
    if let Err(e) = transfer_one(
        source,
        destination,
        src,
        plan,
        prog,
        CopyTuning {
            limit: 0,
            verify: true,
            buf_size: 256 * 1024,
        },
        moving,
        &mut errors,
    ) {
        errors.insert(
            0,
            crate::commands::fs::transfer::events::ItemError::new(source.address(src)?, e.into()),
        );
        // The shared engine's ordinary cleanup is cancellation-gated. Retry
        // only our registered staging entry, with live authorization intact.
        if let Err(cleanup) = destination.cleanup_stage(&plan.write_to) {
            errors.push(crate::commands::fs::transfer::events::ItemError::new(
                destination.address(&plan.write_to)?,
                AppError::from(cleanup)
                    .op("cleanup")
                    .action(crate::errors::ErrorAction::Report),
            ));
        }
    }
    if !errors.is_empty() {
        return Err(AppError::new(code::FS_IO).detail(serde_json::to_string(&errors)?));
    }
    Ok(())
}

pub fn extract(args: &Args) -> AppResult<Value> {
    extract_with_progress(args, &mut progress("extract"))
}

fn extract_with_progress(args: &Args, prog: &mut Prog) -> AppResult<Value> {
    use crate::commands::fs::archive::{
        extract::{self, ExtractJob},
        safety::Limits,
    };
    let archive = args.str("path")?;
    let dest_dir = args.str("to")?;
    let (provider, _) = ScopedProvider::resolve(&dest_dir, args.config)?;
    let source = std::path::PathBuf::from(&archive);
    let job = ExtractJob {
        archive,
        dest_dir,
        base_inner: args.opt_str("inner").unwrap_or_default(),
        names: Vec::new(),
        pw: args.opt_str("password"),
    };
    let (files, errors) = extract::run(job, prog, Limits::load()?, &|path| {
        provider.check(&source.as_path().into())?;
        provider.check(&path.into()).map_err(AppError::from)
    })?;
    if !errors.is_empty() || prog.cancelled() {
        return Err(AppError::new(code::FS_IO).detail(serde_json::to_string(&errors)?));
    }
    Ok(json!({"files": files, "bytes": prog.bytes_done}))
}

pub fn create_archive(args: &Args) -> AppResult<Value> {
    use crate::commands::fs::archive::{pack, safety::Limits};
    let sources: Vec<String> = args
        .raw
        .get("sources")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::new(code::VALIDATION_BAD_INPUT))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| AppError::new(code::VALIDATION_BAD_INPUT))
        })
        .collect::<AppResult<_>>()?;
    let destination = args.str("to")?;
    let (provider, _) = ScopedProvider::resolve(&destination, args.config)?;
    let mut prog = progress("pack");
    let files = pack::run(
        &sources,
        &destination,
        &mut prog,
        Limits::load()?,
        &|path| provider.check(&path.into()).map_err(AppError::from),
    )?;
    Ok(json!({"files":files,"bytes":prog.bytes_done}))
}

#[cfg(test)]
mod tests {
    use super::super::{tools, McpConfig};
    use super::*;
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "hortcmd-mcp-transfer-{}",
                super::super::auth::random_hex(16).unwrap()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(std::fs::canonicalize(path).unwrap())
        }
        fn config(&self) -> McpConfig {
            McpConfig {
                enabled: true,
                allow_write: true,
                confirm_writes: false,
                roots: vec![self.0.to_string_lossy().into_owned()],
                ..Default::default()
            }
        }
        fn call(&self, name: &str, raw: Value) -> Value {
            tools::call("test", name, &raw, &self.config())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    thread_local! {
        static LIVE_CONFIG: std::cell::RefCell<McpConfig> = std::cell::RefCell::new(McpConfig::default());
    }

    fn live_config() -> McpConfig {
        LIVE_CONFIG.with(|config| config.borrow().clone())
    }

    struct CancelOnProgress {
        cancel: Arc<AtomicBool>,
        revoke: Option<&'static str>,
        after_copy: bool,
    }
    impl ProgressSink for CancelOnProgress {
        fn progress(&self, report: OpProgress) {
            if !self.after_copy || report.files_done > 0 {
                if let Some(revoke) = self.revoke {
                    LIVE_CONFIG.with(|config| {
                        let mut config = config.borrow_mut();
                        match revoke {
                            "roots" => config.roots.clear(),
                            "write" => config.allow_write = false,
                            "enabled" => config.enabled = false,
                            _ => unreachable!(),
                        }
                    });
                }
                self.cancel
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    #[test]
    fn cancelled_copy_and_move_clean_only_their_stage_and_report_revoked_cleanup() {
        for moving in [false, true] {
            for tree in [false, true] {
                for after_copy in [false, true] {
                    for revoke in [None, Some("roots"), Some("write"), Some("enabled")] {
                        let f = Fixture::new();
                        let src = f.0.join("source");
                        let dst = f.0.join("destination");
                        if tree {
                            std::fs::create_dir(&src).unwrap();
                        }
                        let payload = if tree { src.join("file") } else { src.clone() };
                        std::fs::write(&payload, b"source payload").unwrap();
                        std::fs::write(&dst, b"old destination").unwrap();
                        let other = f.0.join(".hortcmd-stage-unrelated");
                        std::fs::write(&other, b"unrelated").unwrap();
                        let config = f.config();
                        LIVE_CONFIG.with(|current| *current.borrow_mut() = config.clone());
                        let cancel = Arc::new(AtomicBool::new(false));
                        let (output, _receiver) = tokio::sync::mpsc::channel(8);
                        execution::run(
                            execution::Execution {
                                cancel: cancel.clone(),
                                config: live_config,
                                progress_token: None,
                                output,
                            },
                            || {
                                let (source, src) =
                                    ScopedProvider::resolve(src.to_str().unwrap(), &config)
                                        .unwrap();
                                let (mut destination, dst) =
                                    ScopedProvider::resolve(dst.to_str().unwrap(), &config)
                                        .unwrap();
                                let plan = Plan::staged(&destination, dst.clone(), true).unwrap();
                                let mut prog = progress(if moving { "move" } else { "copy" });
                                prog.sink = Arc::new(CancelOnProgress {
                                    cancel,
                                    revoke,
                                    after_copy,
                                });
                                let error = transfer_planned(
                                    &source,
                                    &mut destination,
                                    &src,
                                    &plan,
                                    &mut prog,
                                    moving,
                                )
                                .unwrap_err();
                                assert_eq!(plan.write_to.native().unwrap().exists(), revoke.is_some(), "moving={moving} tree={tree} after_copy={after_copy} revoke={revoke:?}");
                                if revoke.is_some() {
                                    let detail = serde_json::to_string(&error).unwrap();
                                    assert!(
                                        detail.contains(
                                            plan.write_to.native().unwrap().to_str().unwrap()
                                        ),
                                        "{detail}"
                                    );
                                    assert!(detail.contains("cleanup"), "{detail}");
                                }
                                assert!(source
                                    .remove_file(&crate::vfs::StreamPath::from(&payload))
                                    .is_err());
                                assert!(destination
                                    .rename(&crate::vfs::StreamPath::from(&other), &dst)
                                    .is_err());
                                assert_eq!(
                                    destination
                                        .cleanup_stage(&crate::vfs::StreamPath::from(&other))
                                        .unwrap_err()
                                        .kind(),
                                    std::io::ErrorKind::PermissionDenied
                                );
                            },
                        );
                        assert_eq!(std::fs::read(&payload).unwrap(), b"source payload");
                        assert_eq!(std::fs::read(&dst).unwrap(), b"old destination");
                        assert_eq!(std::fs::read(&other).unwrap(), b"unrelated");
                    }
                }
            }
        }
    }

    #[test]
    fn stage_registration_refuses_existing_paths_and_cleanup_requires_creation() {
        let f = Fixture::new();
        let existing = f.0.join(".hortcmd-stage-existing");
        std::fs::write(&existing, b"keep").unwrap();
        let (mut provider, _) =
            ScopedProvider::resolve(existing.to_str().unwrap(), &f.config()).unwrap();
        assert_eq!(
            provider
                .track_stage(&crate::vfs::StreamPath::from(&existing))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        let stage = f.0.join(".hortcmd-stage-new");
        provider
            .track_stage(&crate::vfs::StreamPath::from(&stage))
            .unwrap();
        // A path appearing without a successful creation by this adapter is not owned.
        std::fs::write(&stage, b"not ours").unwrap();
        assert_eq!(
            provider
                .cleanup_stage(&crate::vfs::StreamPath::from(&stage))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert_eq!(std::fs::read(&stage).unwrap(), b"not ours");
        assert_eq!(std::fs::read(&existing).unwrap(), b"keep");
    }

    #[test]
    fn copies_and_moves_trees_with_explicit_overwrite_and_scope() {
        let f = Fixture::new();
        let src = f.0.join("source");
        let dst = f.0.join("copy");
        let moved = f.0.join("moved");
        std::fs::create_dir_all(src.join("empty")).unwrap();
        std::fs::write(src.join("file"), "payload").unwrap();
        let raw = json!({"from":src,"to":dst});
        assert_eq!(f.call("copy", raw.clone())["isError"], false);
        assert!(dst.join("empty").is_dir());
        assert_eq!(
            std::fs::read_to_string(dst.join("file")).unwrap(),
            "payload"
        );
        std::fs::write(src.join("file"), "changed").unwrap();
        assert_eq!(f.call("copy", raw)["isError"], true);
        assert_eq!(
            std::fs::read_to_string(dst.join("file")).unwrap(),
            "payload"
        );
        assert_eq!(
            f.call("copy", json!({"from":src,"to":dst,"overwrite":true}))["isError"],
            false
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("file")).unwrap(),
            "changed"
        );
        assert_eq!(
            f.call("move", json!({"from":src,"to":moved}))["isError"],
            false
        );
        assert!(!src.exists());
        assert_eq!(
            std::fs::read_to_string(moved.join("file")).unwrap(),
            "changed"
        );
        assert_eq!(
            f.call("copy", json!({"from":moved,"to":moved.join("nested")}))["isError"],
            true
        );
        assert_eq!(
            f.call(
                "move",
                json!({"from":moved,"to":f.0.parent().unwrap().join("outside-scope")})
            )["isError"],
            true
        );
        assert!(moved.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_a_source_tree_cannot_copy_or_delete_its_target() {
        let f = Fixture::new();
        let outside = Fixture::new();
        std::fs::write(outside.0.join("secret"), "secret").unwrap();
        let src = f.0.join("source");
        let dst = f.0.join("destination");
        std::fs::create_dir(&src).unwrap();
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(dst.join("old"), "old").unwrap();
        std::os::unix::fs::symlink(&outside.0, src.join("link")).unwrap();
        let out = f.call("move", json!({"from":src,"to":dst,"overwrite":true}));
        assert_eq!(out["isError"], true, "{out}");
        assert_eq!(std::fs::read_to_string(dst.join("old")).unwrap(), "old");
        assert_eq!(
            std::fs::read_to_string(outside.0.join("secret")).unwrap(),
            "secret"
        );
        assert!(src.exists());
        assert_eq!(
            std::fs::read_dir(&f.0).unwrap().count(),
            2,
            "failed staging is cleaned up"
        );
    }

    struct RevokeExtraction {
        mode: &'static str,
        after_file: bool,
    }
    impl ProgressSink for RevokeExtraction {
        fn progress(&self, report: OpProgress) {
            if (self.after_file && report.files_done > 0)
                || (!self.after_file && !report.file_name.is_empty())
            {
                LIVE_CONFIG.with(|config| {
                    let mut config = config.borrow_mut();
                    match self.mode {
                        "roots" => config.roots.clear(),
                        "write" => config.allow_write = false,
                        "enabled" => config.enabled = false,
                        _ => unreachable!(),
                    }
                });
            }
        }
    }

    #[test]
    fn extraction_rechecks_live_authorization_and_cleans_private_staging() {
        use std::io::Write;
        for mode in ["roots", "write", "enabled"] {
            for after_file in [false, true] {
                let f = Fixture::new();
                let archive = f.0.join("files.zip");
                let destination = f.0.join("output");
                std::fs::create_dir(&destination).unwrap();
                std::fs::write(destination.join("file.txt"), b"old").unwrap();
                let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
                zip.add_directory("empty/", zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.start_file("file.txt", zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(b"replacement").unwrap();
                zip.finish().unwrap();
                let config = f.config();
                LIVE_CONFIG.with(|current| *current.borrow_mut() = config.clone());
                let cancel = Arc::new(AtomicBool::new(false));
                let (output, _receiver) = tokio::sync::mpsc::channel(8);
                execution::run(
                    execution::Execution {
                        cancel: cancel.clone(),
                        config: live_config,
                        progress_token: None,
                        output,
                    },
                    || {
                        let mut prog = progress("extract");
                        prog.sink = Arc::new(RevokeExtraction { mode, after_file });
                        let raw = json!({"path": archive, "to": destination});
                        let error = extract_with_progress(
                            &Args {
                                raw: &raw,
                                config: &config,
                            },
                            &mut prog,
                        )
                        .unwrap_err();
                        assert!(error.to_string().contains(code::FS_PERMISSION), "{error}");
                        assert!(
                            !prog.cancelled(),
                            "revocation must abort without a cancel request"
                        );
                    },
                );
                assert_eq!(std::fs::read(destination.join("file.txt")).unwrap(), b"old");
                assert_eq!(
                    std::fs::read_dir(&destination).unwrap().count(),
                    1,
                    "private stage must be removed"
                );
            }
        }
    }

    #[test]
    fn extraction_uses_the_shared_archive_service_and_checks_both_paths() {
        use std::io::Write;
        let f = Fixture::new();
        let archive = f.0.join("files.zip");
        let dest = f.0.join("output");
        std::fs::create_dir(&dest).unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        zip.start_file("file.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"archive payload").unwrap();
        zip.finish().unwrap();
        let out = f.call("extract", json!({"path":archive,"to":dest}));
        assert_eq!(out["isError"], false, "{out}");
        assert_eq!(
            std::fs::read(dest.join("file.txt")).unwrap(),
            b"archive payload"
        );
        let outside = Fixture::new();
        assert_eq!(
            f.call("extract", json!({"path":archive,"to":outside.0}))["isError"],
            true
        );
        assert_eq!(std::fs::read_dir(&outside.0).unwrap().count(), 0);
    }

    #[test]
    fn creates_archives_via_the_shared_service_and_scopes_every_source() {
        let f = Fixture::new();
        let source = f.0.join("payload");
        let destination = f.0.join("result.zip");
        std::fs::write(&source, b"packed").unwrap();
        let out = f.call(
            "create_archive",
            json!({"sources":[source],"to":destination}),
        );
        assert_eq!(out["isError"], false, "{out}");
        let previous = std::fs::read(&destination).unwrap();
        let outside = Fixture::new();
        std::fs::write(outside.0.join("secret"), b"secret").unwrap();
        let out = f.call(
            "create_archive",
            json!({"sources":[source, outside.0],"to":destination}),
        );
        assert_eq!(out["isError"], true);
        assert_eq!(std::fs::read(destination).unwrap(), previous);
    }
}
