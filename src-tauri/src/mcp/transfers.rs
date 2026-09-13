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
    let (destination, dst) = ScopedProvider::resolve(&to, args.config)?;
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
    let plan = Plan::staged(&destination, dst, exists);
    let mut prog = progress(if moving { "move" } else { "copy" });
    let mut errors = Vec::new();
    if let Err(e) = transfer_one(
        &source,
        &destination,
        &src,
        &plan,
        &mut prog,
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
            crate::commands::fs::transfer::events::ItemError::new(&from, e.into()),
        );
    }
    if !errors.is_empty() {
        return Err(AppError::new(code::FS_IO).detail(serde_json::to_string(&errors)?));
    }
    Ok(
        json!({ "from": from, "to": to, "files": prog.files_done, "bytes": prog.bytes_done, "moved": moving }),
    )
}

pub fn extract(args: &Args) -> AppResult<Value> {
    use crate::commands::fs::archive::{
        extract::{self, ExtractJob},
        safety::Limits,
    };
    let archive = args.str("path")?;
    let dest_dir = args.str("to")?;
    let job = ExtractJob {
        archive,
        dest_dir,
        base_inner: args.opt_str("inner").unwrap_or_default(),
        names: Vec::new(),
        pw: args.opt_str("password"),
    };
    let mut prog = progress("extract");
    let (files, errors) = extract::run(job, &mut prog, Limits::load()?)?;
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
        &|path| provider.check(path).map_err(AppError::from),
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
