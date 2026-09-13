// ============================================================
// What a client may ask for, and what happens when it does (TICKET-041).
//
// One table. Each entry declares its name, its tier, what it is for and
// the shape of its arguments, and every call goes through the same three
// gates in the same order:
//
//   1. **tier** — is this kind of call allowed at all?
//   2. **scope** — is every address it names inside an allowed root?
//   3. **confirmation** — for write and delete, does the user say yes?
//
// The order matters. A refusal must be able to say *which* gate stopped
// it: "the write tier is off" is something the user can fix in the
// settings, "outside the allowed roots" is something else, and neither
// may be confusable with "not found" — a refusal that looks like an
// absence turns the allowlist into a way to probe the file system.
//
// Every handler calls the same VFS the application does. There is no
// second implementation of anything here, and no handler reaches for a
// Tauri command: the shared layer is `vfs::resolve` and the provider
// behind it.
// ============================================================

use super::{audit, confirm, scope, McpConfig, McpTier};
use crate::errors::AppResult;
use crate::vfs;
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;

/// One callable tool.
pub struct Tool {
    pub name: &'static str,
    pub tier: McpTier,
    /// Shown to the model — it decides from this whether to call it.
    pub description: &'static str,
    /// JSON Schema for the arguments.
    pub schema: fn() -> Value,
    /// Runs it. The address it names, for the audit log, comes back too.
    pub run: fn(&Args) -> AppResult<Value>,
    /// What the confirmation dialog says this call will do.
    pub summary: fn(&Args) -> String,
}

/// The arguments of one call, with the accessors a handler needs.
pub struct Args<'a> {
    pub raw: &'a Value,
    pub config: &'a McpConfig,
}

impl Args<'_> {
    pub fn str(&self, key: &str) -> AppResult<String> {
        self.raw
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| bad_input(format!("\"{}\" is required and must be a string", key)))
    }

    pub fn opt_str(&self, key: &str) -> Option<String> {
        self.raw
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    pub fn bool(&self, key: &str, fallback: bool) -> bool {
        self.raw
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(fallback)
    }

    pub fn u64(&self, key: &str, fallback: u64) -> u64 {
        self.raw
            .get(key)
            .and_then(Value::as_u64)
            .unwrap_or(fallback)
    }

    /// The addresses this call names, for the scope check and the log.
    pub fn addresses(&self, keys: &[&str]) -> Vec<String> {
        let mut out = Vec::new();
        for key in keys {
            match self.raw.get(*key) {
                Some(Value::String(s)) => out.push(s.clone()),
                Some(Value::Array(items)) => {
                    out.extend(items.iter().filter_map(Value::as_str).map(str::to_string));
                }
                _ => {}
            }
        }
        out
    }
}

fn bad_input(detail: impl std::fmt::Display) -> crate::errors::AppError {
    crate::errors::AppError::new(crate::errors::code::VALIDATION_BAD_INPUT)
        .op("mcp-tool")
        .detail(detail)
}

// ---------- Schemas ----------

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn address_field(what: &str) -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "description": format!(
            "{} — a local path, or a VFS address of a connection the user has open \
             (sftp://user@host/path, smb://…, webdav://…, ftp://…).",
            what
        ),
    })
}

// ---------- Read tier ----------

fn list_dir(args: &Args) -> AppResult<Value> {
    let path = args.str("path")?;
    let (provider, resolved) = vfs::resolve(&path)?;
    let listing = provider.list(&resolved)?;
    let entries: Vec<Value> = listing
        .entries
        .iter()
        .filter(|e| args.bool("include_hidden", false) || !e.hidden)
        .map(|e| {
            json!({
                "name": e.name,
                "isDir": e.is_dir,
                "isSymlink": e.is_symlink,
                "size": e.size,
                "modified": e.modified,
            })
        })
        .collect();
    Ok(json!({ "path": listing.path, "count": entries.len(), "entries": entries }))
}

fn stat(args: &Args) -> AppResult<Value> {
    let path = args.str("path")?;
    let (provider, resolved) = vfs::resolve(&path)?;
    let st = provider.stat(Path::new(&resolved))?;
    Ok(json!({ "path": path, "isDir": st.is_dir, "size": st.len }))
}

fn read_file(args: &Args) -> AppResult<Value> {
    let path = args.str("path")?;
    let (provider, resolved) = vfs::resolve(&path)?;
    let target = Path::new(&resolved);

    // The size is checked *before* a byte is read, so an oversized file
    // costs a `stat` rather than a transfer that is thrown away.
    let st = provider.stat(target)?;
    if st.is_dir {
        return Err(bad_input(format!("{} is a directory", path)));
    }
    let cap = args.config.max_read_bytes;
    if st.len > cap {
        return Err(
            crate::errors::AppError::new(crate::errors::code::FS_TOO_LARGE).detail(format!(
                "{} is {} bytes, over the {}-byte limit for this interface",
                path, st.len, cap
            )),
        );
    }

    let mut buf = Vec::with_capacity(st.len as usize);
    provider
        .open_read(target)?
        // One more byte than the cap allows, so a file that grew between
        // the stat and the read is still bounded.
        .take(cap + 1)
        .read_to_end(&mut buf)?;
    if buf.len() as u64 > cap {
        return Err(
            crate::errors::AppError::new(crate::errors::code::FS_TOO_LARGE)
                .detail(format!("{} grew past the size limit while reading", path)),
        );
    }

    match String::from_utf8(buf) {
        Ok(text) => Ok(json!({ "path": path, "encoding": "utf-8", "text": text })),
        Err(e) => {
            // Not text. Handing back base64 keeps the tool honest about
            // what it read instead of returning mangled characters.
            let bytes = e.into_bytes();
            Ok(json!({
                "path": path,
                "encoding": "base64",
                "size": bytes.len(),
                "data": base64(&bytes),
            }))
        }
    }
}

/// Minimal base64, so a binary answer needs no new dependency.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn checksums(args: &Args) -> AppResult<Value> {
    let path = args.str("path")?;
    let sums = crate::commands::fs::props::read_file_checksums(path.clone())?;
    Ok(json!({ "path": path, "md5": sums.md5, "sha1": sums.sha1, "sha256": sums.sha256 }))
}

fn properties(args: &Args) -> AppResult<Value> {
    let path = args.str("path")?;
    let props = crate::commands::fs::props::read_file_props(path)?;
    Ok(serde_json::to_value(props).unwrap_or(Value::Null))
}

fn search(args: &Args) -> AppResult<Value> {
    use crate::commands::fs::search::{HitSink, Limits, SearchHit, SearchOptions};
    use std::sync::Mutex;

    let root = args.str("root")?;
    let options = SearchOptions {
        mode: args.opt_str("mode").unwrap_or_else(|| "files".into()),
        name: args.opt_str("name").unwrap_or_default(),
        name_regex: args.bool("name_regex", false),
        content: args.opt_str("content").unwrap_or_default(),
        content_regex: args.bool("content_regex", false),
        case_sensitive: args.bool("case_sensitive", false),
        ignore_dirs: Vec::new(),
        min_size: args.u64("min_size", 0),
    };

    /// Collects the hits instead of streaming them to a dialog, and
    /// stops once the client's own limit is reached.
    struct Collect {
        hits: Mutex<Vec<SearchHit>>,
        max: usize,
    }
    impl HitSink for Collect {
        fn send(&self, batch: Vec<SearchHit>) -> bool {
            let mut hits = self.hits.lock().expect("mcp search hits");
            hits.extend(batch);
            super::execution::progress(hits.len() as u64, None);
            hits.len() < self.max
        }
    }

    let max = args.u64("limit", 200).clamp(1, 5_000) as usize;
    let sink = Collect {
        hits: Mutex::new(Vec::new()),
        max,
    };
    let summary = crate::commands::fs::search::run_search(
        std::path::PathBuf::from(&root),
        options,
        super::execution::cancel_flag(),
        &Limits::DEFAULT,
        &sink,
    )?;

    let mut hits = sink.hits.into_inner().expect("mcp search hits");
    hits.truncate(max);
    Ok(json!({
        "root": root,
        "files": summary.files,
        "dirs": summary.dirs,
        "unreadable": summary.unreadable,
        "status": serde_json::to_value(summary.status).unwrap_or(Value::Null),
        "truncated": hits.len() >= max,
        "hits": serde_json::to_value(&hits).unwrap_or(Value::Null),
    }))
}

fn list_archive(args: &Args) -> AppResult<Value> {
    let listing = crate::commands::fs::archive::read_archive(
        args.str("path")?,
        args.opt_str("inner").unwrap_or_default(),
        None,
    )?;
    Ok(serde_json::to_value(listing)?)
}

fn volume_info(args: &Args) -> AppResult<Value> {
    let path = args.str("path")?;
    crate::vfs::caps::require(&path, crate::vfs::caps::Capability::VolumeInfo)?;
    let (_, path) = vfs::resolve(&path)?;
    Ok(serde_json::to_value(crate::commands::fs::volume::find(
        Path::new(&path),
    ))?)
}

fn git_status(args: &Args) -> AppResult<Value> {
    let path = args.str("path")?;
    crate::vfs::caps::require(&path, crate::vfs::caps::Capability::GitStatus)?;
    let (_, path) = vfs::resolve(&path)?;
    Ok(serde_json::to_value(
        crate::commands::fs::git::compute_git_status(&path),
    )?)
}

fn compare_dirs(args: &Args) -> AppResult<Value> {
    let left = args.str("left")?;
    let right = args.str("right")?;
    crate::vfs::caps::require_pair(
        &left,
        crate::vfs::caps::Capability::CompareDirs,
        &right,
        crate::vfs::caps::Capability::CompareDirs,
    )?;
    let (_, left) = vfs::resolve(&left)?;
    let (_, right) = vfs::resolve(&right)?;
    let entries = std::cell::RefCell::new(Vec::new());
    let truncated = crate::commands::fs::compare::walk_compare(
        Path::new(&left),
        Path::new(&right),
        "",
        args.bool("recursive", false),
        &|batch| entries.borrow_mut().extend(batch),
        &mut 0,
    );
    Ok(json!({ "entries": entries.into_inner(), "truncated": truncated }))
}

// ---------- Write tier ----------

fn make_dir(args: &Args) -> AppResult<Value> {
    let path = args.str("path")?;
    let (provider, resolved) = vfs::resolve(&path)?;
    provider.create_dir_all(Path::new(&resolved))?;
    Ok(json!({ "path": path, "created": true }))
}

fn write_file(args: &Args) -> AppResult<Value> {
    let path = args.str("path")?;
    let contents = args.str("contents")?;
    let (provider, resolved) = vfs::resolve(&path)?;
    let target = Path::new(&resolved);
    if !args.bool("overwrite", false) && provider.exists(target) {
        return Err(bad_input(format!(
            "{} exists; pass overwrite: true to replace it",
            path
        )));
    }
    provider.write(target, contents.as_bytes())?;
    Ok(json!({ "path": path, "bytes": contents.len() }))
}

fn rename(args: &Args) -> AppResult<Value> {
    let from = args.str("from")?;
    let to = args.str("to")?;
    let (provider, from_resolved) = vfs::resolve(&from)?;
    let (destination, to_resolved) = vfs::resolve(&to)?;
    if provider.store_id() != destination.store_id() {
        return Err(bad_input("rename requires the same store"));
    }
    if provider.exists(Path::new(&to_resolved)) {
        return Err(bad_input(format!("{} already exists", to)));
    }
    provider.rename(Path::new(&from_resolved), Path::new(&to_resolved))?;
    Ok(json!({ "from": from, "to": to }))
}

// ---------- Delete tier ----------

fn trash(args: &Args) -> AppResult<Value> {
    let path = args.str("path")?;
    crate::vfs::caps::require(&path, crate::vfs::caps::Capability::Trash)?;
    let (_, resolved) = vfs::resolve(&path)?;
    trash::delete(&resolved).map_err(|e| {
        crate::errors::AppError::new(crate::errors::code::FS_IO)
            .op("mcp-trash")
            .detail(e)
    })?;
    Ok(json!({ "path": path, "trashed": true }))
}

fn delete(args: &Args) -> AppResult<Value> {
    if !args.config.allow_permanent_delete {
        return Err(bad_input(
            "permanent deletion is switched off for this interface; use trash instead",
        ));
    }
    let path = args.str("path")?;
    let (provider, resolved) = vfs::resolve(&path)?;
    let target = Path::new(&resolved);
    if provider.is_dir(target) {
        provider.remove_dir_all(target)?;
    } else {
        provider.remove_file(target)?;
    }
    Ok(json!({ "path": path, "deleted": true }))
}

// ---------- The table ----------

pub fn all() -> &'static [Tool] {
    &[
        Tool {
            name: "list_dir",
            tier: McpTier::Read,
            description: "List the entries of a directory.",
            schema: || {
                schema(
                    json!({
                        "path": address_field("The directory to list"),
                        "include_hidden": {
                            "type": "boolean",
                            "description": "Include dotfiles and system entries (default false).",
                        },
                    }),
                    &["path"],
                )
            },
            run: list_dir,
            summary: |a| format!("list {}", a.opt_str("path").unwrap_or_default()),
        },
        Tool {
            name: "stat",
            tier: McpTier::Read,
            description: "Whether a path exists, whether it is a directory, and its size.",
            schema: || schema(json!({ "path": address_field("The entry") }), &["path"]),
            run: stat,
            summary: |a| format!("stat {}", a.opt_str("path").unwrap_or_default()),
        },
        Tool {
            name: "read_file",
            tier: McpTier::Read,
            description:
                "Read a file. Text comes back as text; anything else as base64. \
                 Files over the configured size limit are refused rather than truncated.",
            schema: || schema(json!({ "path": address_field("The file") }), &["path"]),
            run: read_file,
            summary: |a| format!("read {}", a.opt_str("path").unwrap_or_default()),
        },
        Tool {
            name: "checksums",
            tier: McpTier::Read,
            description: "MD5, SHA-1 and SHA-256 of a local file.",
            schema: || schema(json!({ "path": address_field("The file") }), &["path"]),
            run: checksums,
            summary: |a| format!("checksum {}", a.opt_str("path").unwrap_or_default()),
        },
        Tool {
            name: "properties",
            tier: McpTier::Read,
            description:
                "Detailed properties of a local entry: times, permissions, owner, \
                 and where the platform provides them, Git status and tags.",
            schema: || schema(json!({ "path": address_field("The entry") }), &["path"]),
            run: properties,
            summary: |a| format!("properties of {}", a.opt_str("path").unwrap_or_default()),
        },
        Tool {
            name: "search",
            tier: McpTier::Read,
            description:
                "Search a local tree by name and optionally by content. Modes: files, \
                 duplicates, empty_dirs, large_files.",
            schema: || {
                schema(
                    json!({
                        "root": address_field("The folder to search in"),
                        "mode": {
                            "type": "string",
                            "enum": ["files", "duplicates", "empty_dirs", "large_files"],
                        },
                        "name": { "type": "string", "description": "Glob, or regex with name_regex." },
                        "name_regex": { "type": "boolean" },
                        "content": { "type": "string", "description": "Text to find inside files." },
                        "content_regex": { "type": "boolean" },
                        "case_sensitive": { "type": "boolean" },
                        "min_size": { "type": "integer", "description": "Bytes, for large_files." },
                        "limit": { "type": "integer", "description": "Most hits to return (default 200)." },
                    }),
                    &["root"],
                )
            },
            run: search,
            summary: |a| format!("search {}", a.opt_str("root").unwrap_or_default()),
        },
        Tool {
            name: "list_archive", tier: McpTier::Read,
            description: "List a local archive without extracting it.",
            schema: || schema(json!({ "path": address_field("Archive"), "inner": { "type": "string" } }), &["path"]),
            run: list_archive, summary: |_| "list archive".into(),
        },
        Tool {
            name: "volume_info", tier: McpTier::Read,
            description: "Capacity and free space of a local volume.",
            schema: || schema(json!({ "path": address_field("Volume path") }), &["path"]),
            run: volume_info, summary: |_| "volume information".into(),
        },
        Tool {
            name: "git_status", tier: McpTier::Read,
            description: "Git status at a local directory.",
            schema: || schema(json!({ "path": address_field("Directory") }), &["path"]),
            run: git_status, summary: |_| "Git status".into(),
        },
        Tool {
            name: "compare_dirs", tier: McpTier::Read,
            description: "Compare local directories by size and modification time.",
            schema: || schema(json!({ "left": address_field("Left directory"), "right": address_field("Right directory"), "recursive": { "type": "boolean" } }), &["left", "right"]),
            run: compare_dirs, summary: |_| "compare directories".into(),
        },
        Tool {
            name: "make_dir",
            tier: McpTier::Write,
            description: "Create a directory, including any missing parents.",
            schema: || schema(json!({ "path": address_field("The directory to create") }), &["path"]),
            run: make_dir,
            summary: |a| format!("create the folder {}", a.opt_str("path").unwrap_or_default()),
        },
        Tool {
            name: "write_file",
            tier: McpTier::Write,
            description: "Write text to a file. Refuses to replace an existing file unless overwrite is true.",
            schema: || {
                schema(
                    json!({
                        "path": address_field("The file to write"),
                        "contents": { "type": "string" },
                        "overwrite": { "type": "boolean" },
                    }),
                    &["path", "contents"],
                )
            },
            run: write_file,
            summary: |a| format!("write {}", a.opt_str("path").unwrap_or_default()),
        },
        Tool {
            name: "rename",
            tier: McpTier::Write,
            description: "Rename or move an entry within one store. Never replaces an existing target.",
            schema: || {
                schema(
                    json!({
                        "from": address_field("The entry to rename"),
                        "to": address_field("The new name or location"),
                    }),
                    &["from", "to"],
                )
            },
            run: rename,
            summary: |a| {
                format!(
                    "rename {} to {}",
                    a.opt_str("from").unwrap_or_default(),
                    a.opt_str("to").unwrap_or_default()
                )
            },
        },
        Tool {
            name: "trash",
            tier: McpTier::Delete,
            description: "Move a local entry to the system trash, where the user can get it back.",
            schema: || schema(json!({ "path": address_field("The entry") }), &["path"]),
            run: trash,
            summary: |a| format!("move {} to the trash", a.opt_str("path").unwrap_or_default()),
        },
        Tool {
            name: "copy", tier: McpTier::Write,
            description: "Copy a file or directory tree through the active VFS connections. The destination is an exact new path with an existing parent; overwrite replaces it after a verified staged copy.",
            schema: transfer_schema, run: super::transfers::copy,
            summary: |a| format!("copy {} to {}", a.opt_str("from").unwrap_or_default(), a.opt_str("to").unwrap_or_default()),
        },
        Tool {
            name: "move", tier: McpTier::Write,
            description: "Move a file or directory tree, removing the source only after a verified staged copy is published. The destination parent must exist.",
            schema: transfer_schema, run: super::transfers::move_entry,
            summary: |a| format!("move {} to {}", a.opt_str("from").unwrap_or_default(), a.opt_str("to").unwrap_or_default()),
        },
        Tool {
            name: "extract", tier: McpTier::Write,
            description: "Extract a local archive into an existing local folder using validated entries, configured limits and rollback staging.",
            schema: || schema(json!({ "path": address_field("Local archive"), "to": address_field("Destination folder"), "inner": {"type":"string"}, "password": {"type":"string"} }), &["path", "to"]),
            run: super::transfers::extract,
            summary: |a| format!("extract {} into {}", a.opt_str("path").unwrap_or_default(), a.opt_str("to").unwrap_or_default()),
        },
        Tool {
            name: "create_archive", tier: McpTier::Write,
            description: "Create a local ZIP from files and directory trees. Existing archives are replaced only after successful completion; links are refused and empty directories are preserved.",
            schema: || schema(json!({ "sources": {"type":"array", "items": address_field("Local source"), "minItems":1}, "to": address_field("Destination ZIP") }), &["sources","to"]),
            run: super::transfers::create_archive,
            summary: |a| format!("create ZIP {} from {}", a.opt_str("to").unwrap_or_default(), a.raw.get("sources").unwrap_or(&Value::Null)),
        },
        Tool {
            name: "delete",
            tier: McpTier::Delete,
            description:
                "Delete permanently. Off unless the user has separately allowed it; \
                 prefer trash, which can be undone.",
            schema: || schema(json!({ "path": address_field("The entry") }), &["path"]),
            run: delete,
            summary: |a| {
                format!(
                    "PERMANENTLY delete {}",
                    a.opt_str("path").unwrap_or_default()
                )
            },
        },
    ]
}

/// The tool by that name.
pub fn find(name: &str) -> Option<&'static Tool> {
    all().iter().find(|t| t.name == name)
}

/// The `tools/list` payload, with the tiers the configuration allows.
///
/// A tool the user has switched off is *not listed*: a model that cannot
/// see `delete` does not try to call it, which is a better experience
/// than a refusal every time — and the refusal still exists for a client
/// that calls it anyway.
pub fn listing(config: &McpConfig) -> Value {
    let tools: Vec<Value> = all()
        .iter()
        .filter(|t| config.allows(t.tier))
        .filter(|t| t.name != "delete" || config.allow_permanent_delete)
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": (t.schema)(),
            })
        })
        .collect();
    json!({ "tools": tools })
}

/// The addresses each tool's arguments name, for the scope check.
fn address_keys(tool: &str) -> &'static [&'static str] {
    match tool {
        "search" => &["root"],
        "compare_dirs" => &["left", "right"],
        "rename" | "copy" | "move" => &["from", "to"],
        "extract" => &["path", "to"],
        "create_archive" => &["sources", "to"],
        _ => &["path"],
    }
}

fn transfer_schema() -> Value {
    schema(
        json!({"from": address_field("Source entry"), "to": address_field("Exact destination path"), "overwrite": {"type": "boolean"}}),
        &["from", "to"],
    )
}

/// Runs one call through all three gates.
#[cfg(test)]
pub fn call(client: &str, name: &str, arguments: &Value, config: &McpConfig) -> Value {
    call_session(client, client, name, arguments, config)
}

pub fn call_session(
    session: &str,
    client: &str,
    name: &str,
    arguments: &Value,
    config: &McpConfig,
) -> Value {
    use super::protocol::{tool_value, typed_tool_error};

    let Some(tool) = find(name) else {
        audit::record(
            client,
            name,
            None,
            audit::McpOutcome::Denied,
            Some("no such tool".into()),
        );
        return typed_tool_error("mcp.unknownTool", format!("no tool named {}", name));
    };

    let args = Args {
        raw: arguments,
        config,
    };
    let addresses = args.addresses(address_keys(name));
    let target = addresses.first().map(String::as_str);

    // 1. The tier.
    if !config.allows(tool.tier) {
        let why = "that kind of call is switched off for this interface";
        audit::record(
            client,
            name,
            target,
            audit::McpOutcome::Denied,
            Some(why.into()),
        );
        return typed_tool_error("mcp.notPermitted", why);
    }

    if let Err(error) = super::schema::validate(&(tool.schema)(), arguments) {
        audit::record(
            client,
            name,
            target,
            audit::McpOutcome::Denied,
            Some(error.clone()),
        );
        return typed_tool_error("validation.badInput", error);
    }

    // 2. The scope — every address the call names, not just the first.
    for address in &addresses {
        if let Err(e) = scope::require_in_scope(&config.roots, address) {
            let why = "outside the folders this interface may reach";
            audit::record(
                client,
                name,
                Some(address),
                audit::McpOutcome::Denied,
                Some(e.to_string()),
            );
            // Deliberately *not* "not found": a refusal that looks like
            // an absence turns the allowlist into a way to probe.
            return typed_tool_error("mcp.scopeDenied", format!("{}: {}", address, why));
        }
    }

    // 3. The user, for anything that changes something.
    let needs_confirmation = config.confirm_writes && tool.tier != McpTier::Read;
    if needs_confirmation {
        match confirm::ask(session, client, name, &(tool.summary)(&args)) {
            confirm::Answer::Approved => {}
            confirm::Answer::Declined => {
                audit::record(
                    client,
                    name,
                    target,
                    audit::McpOutcome::Declined,
                    Some("the user declined".into()),
                );
                return typed_tool_error("mcp.declined", "the user declined this in hortcmd");
            }
            confirm::Answer::Unavailable => {
                audit::record(
                    client,
                    name,
                    target,
                    audit::McpOutcome::Denied,
                    Some("no window to ask in".into()),
                );
                return typed_tool_error(
                    "mcp.needsConfirmation",
                    "hortcmd could not ask the user, so nothing was done",
                );
            }
        }
    }

    let live_config = super::execution::live_config();
    let current = live_config.as_ref().unwrap_or(config);
    if (live_config.is_some() && !current.enabled)
        || !current.allows(tool.tier)
        || (name == "delete" && !current.allow_permanent_delete)
        || (!needs_confirmation && current.confirm_writes && tool.tier != McpTier::Read)
    {
        audit::record(
            client,
            name,
            target,
            audit::McpOutcome::Denied,
            Some("permission changed before execution".into()),
        );
        return typed_tool_error(
            "mcp.notPermitted",
            "permission changed before execution; submit a new request",
        );
    }
    for address in &addresses {
        if scope::require_in_scope(&current.roots, address).is_err() {
            audit::record(
                client,
                name,
                Some(address),
                audit::McpOutcome::Denied,
                Some("scope changed before execution".into()),
            );
            return typed_tool_error("mcp.scopeDenied", "scope changed before execution");
        }
    }
    if super::execution::cancel_flag().load(std::sync::atomic::Ordering::Relaxed) {
        audit::record(
            client,
            name,
            target,
            audit::McpOutcome::Denied,
            Some("request cancelled before execution".into()),
        );
        return typed_tool_error("cancelled", "request cancelled before execution");
    }
    let args = Args {
        raw: arguments,
        config: current,
    };
    match (tool.run)(&args) {
        Ok(value) => {
            audit::record(
                client,
                name,
                target,
                if needs_confirmation {
                    audit::McpOutcome::Confirmed
                } else {
                    audit::McpOutcome::Allowed
                },
                None,
            );
            tool_value(value)
        }
        Err(e) => {
            audit::record(
                client,
                name,
                target,
                audit::McpOutcome::Failed,
                Some(e.to_string()),
            );
            // The typed code travels with the message: a client can tell
            // "not found" from "not connected" without parsing prose.
            typed_tool_error(&e.code, e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revoked_permission_and_cancelled_requests_cannot_create_files() {
        use std::sync::{atomic::AtomicBool, Arc};
        fn disabled() -> McpConfig {
            McpConfig::default()
        }
        fn enabled() -> McpConfig {
            McpConfig {
                enabled: true,
                allow_write: true,
                confirm_writes: false,
                roots: vec![std::env::temp_dir().to_string_lossy().into_owned()],
                ..McpConfig::default()
            }
        }
        let path = std::env::temp_dir().join(format!(
            "hortcmd-revoked-write-{}",
            super::super::auth::random_hex(16).unwrap()
        ));
        for (config_fn, cancelled, expected) in [
            (disabled as fn() -> McpConfig, false, "permission changed"),
            (enabled as fn() -> McpConfig, true, "cancelled"),
        ] {
            let (output, _input) = tokio::sync::mpsc::channel(1);
            let execution = super::super::execution::Execution {
                cancel: Arc::new(AtomicBool::new(cancelled)),
                config: config_fn,
                output,
                progress_token: None,
            };
            let out = super::super::execution::run(execution, || {
                call(
                    "test",
                    "write_file",
                    &json!({"path": path, "contents": "x"}),
                    &enabled(),
                )
            });
            assert_eq!(out["isError"], true);
            assert!(out["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains(expected));
            assert!(!path.exists());
        }
    }

    fn config() -> McpConfig {
        McpConfig {
            roots: vec!["/tmp".into()],
            ..McpConfig::default()
        }
    }

    #[test]
    fn every_tool_has_a_schema_an_object_schema() {
        for tool in all() {
            let s = (tool.schema)();
            assert_eq!(s["type"], "object", "{}", tool.name);
            assert_eq!(
                s["additionalProperties"], false,
                "{} must not accept unknown arguments",
                tool.name
            );
            assert!(s["required"].is_array(), "{}", tool.name);
            assert!(!tool.description.is_empty(), "{}", tool.name);
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let mut names: Vec<&str> = all().iter().map(|t| t.name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before);
    }

    #[test]
    fn read_can_be_disabled_independently_and_errors_have_stable_codes() {
        let mut c = config();
        c.allow_read = false;
        assert_eq!(listing(&c)["tools"], json!([]));
        assert_eq!(
            call("test", "list_dir", &json!({"path":"/tmp"}), &c)["structuredContent"]["error"]
                ["code"],
            "mcp.notPermitted"
        );
        c.allow_read = true;
        assert_eq!(
            call("test", "list_dir", &json!({"path":"/etc"}), &c)["structuredContent"]["error"]
                ["code"],
            "mcp.scopeDenied"
        );
        assert_eq!(
            call("test", "search", &json!({"root":"/tmp","limit":-1}), &c)["structuredContent"]
                ["error"]["code"],
            "validation.badInput"
        );
    }

    /// A tool the user switched off is not offered at all — a model that
    /// cannot see it does not try.
    #[test]
    fn the_listing_shows_only_what_is_allowed() {
        let names = |c: &McpConfig| -> Vec<String> {
            listing(c)["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["name"].as_str().unwrap().to_string())
                .collect()
        };

        let read_only = names(&config());
        assert!(read_only.contains(&"list_dir".to_string()));
        assert!(!read_only.contains(&"write_file".to_string()));
        assert!(!read_only.contains(&"trash".to_string()));

        let mut c = config();
        c.allow_write = true;
        assert!(names(&c).contains(&"write_file".to_string()));
        assert!(!names(&c).contains(&"trash".to_string()));

        c.allow_delete = true;
        let with_delete = names(&c);
        assert!(with_delete.contains(&"trash".to_string()));
        // Permanent deletion needs its own switch even inside its tier.
        assert!(!with_delete.contains(&"delete".to_string()));

        c.allow_permanent_delete = true;
        assert!(names(&c).contains(&"delete".to_string()));
    }

    #[test]
    fn a_tool_that_does_not_exist_is_an_error_not_a_panic() {
        let out = call("test", "rm_rf", &json!({}), &config());
        assert_eq!(out["isError"], true);
    }

    /// The order of the gates is visible in what a refusal says.
    #[test]
    fn the_tier_is_checked_before_the_scope() {
        // A write outside the roots, with the write tier off: the answer
        // must be about the tier, because that is the first gate.
        let out = call(
            "test",
            "write_file",
            &json!({ "path": "/etc/x", "contents": "" }),
            &config(),
        );
        assert_eq!(out["isError"], true);
        let text = out["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("switched off"), "{}", text);
    }

    #[test]
    fn a_path_outside_the_roots_is_refused_and_does_not_look_like_absence() {
        let out = call("test", "list_dir", &json!({ "path": "/etc" }), &config());
        assert_eq!(out["isError"], true);
        let text = out["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("may reach"), "{}", text);
        assert!(
            !text.contains("not found"),
            "a refusal must not look like an absence"
        );
    }

    #[test]
    fn every_address_an_argument_names_is_checked_not_only_the_first() {
        let mut c = config();
        c.allow_write = true;
        c.confirm_writes = false;
        // The source is inside the root, the destination is not.
        let out = call(
            "test",
            "rename",
            &json!({ "from": "/tmp/a", "to": "/etc/b" }),
            &c,
        );
        assert_eq!(out["isError"], true);
        assert!(out["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("/etc/b"));
    }

    #[test]
    fn base64_matches_the_reference_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn arguments_are_read_with_their_types_and_defaults() {
        let raw = json!({ "path": "/a", "flag": true, "n": 7 });
        let c = config();
        let args = Args {
            raw: &raw,
            config: &c,
        };
        assert_eq!(args.str("path").unwrap(), "/a");
        assert!(args.str("missing").is_err());
        assert!(args.bool("flag", false));
        assert!(args.bool("absent", true));
        assert_eq!(args.u64("n", 0), 7);
        assert_eq!(args.u64("absent", 5), 5);
    }

    #[test]
    fn a_list_argument_contributes_every_address_it_holds() {
        let raw = json!({ "paths": ["/a", "/b"], "path": "/c" });
        let c = config();
        let args = Args {
            raw: &raw,
            config: &c,
        };
        assert_eq!(args.addresses(&["path", "paths"]), vec!["/c", "/a", "/b"]);
    }
}
