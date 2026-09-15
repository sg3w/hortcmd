//! Read-only transfer plans. Reviewed data is never an execution capability.
#[cfg(test)]
mod tests;
pub mod types;
mod walk;
use super::recovery::types::{RecoveryKind, RecoveryLocation, RecoverySettings};
use crate::errors::{code, AppError, AppResult};
use crate::vfs::StreamPath;
use crate::vfs::{
    self,
    caps::{self, Capability},
    Provider,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Component;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use types::{OperationMapping, OperationPlan, PlanIssue, SpaceEstimate};

fn invalid() -> AppError {
    AppError::new(code::VALIDATION_BAD_INPUT).op("transfer-preflight")
}
fn checked(value: Option<u64>) -> AppResult<u64> {
    value.ok_or_else(invalid)
}
pub fn space(
    kind: &RecoveryKind,
    bytes: u64,
    old: u64,
    files: u32,
    safe: bool,
    verify: bool,
) -> AppResult<SpaceEstimate> {
    // ZIP may grow on incompressible input. The conservative bound includes
    // per-entry headers/names and deflate block overhead; never assume compression.
    let output = if *kind == RecoveryKind::Pack {
        checked(
            bytes
                .checked_add(bytes / 100 + 1)
                .and_then(|n| n.checked_add(u64::from(files).checked_mul(65_536)?))
                .and_then(|n| n.checked_add(65_536)),
        )?
    } else {
        bytes
    };
    Ok(SpaceEstimate {
        additional_bytes: output,
        staging_bytes: if safe { output } else { 0 },
        backup_bytes: if safe { old } else { 0 },
        peak_footprint_bytes: checked(output.checked_add(old))?,
        verification_read_bytes: if verify {
            checked(bytes.checked_mul(2))?
        } else {
            0
        },
    })
}
fn normalize(path: &std::path::Path) -> AppResult<std::path::PathBuf> {
    if !path.is_absolute() {
        return Err(invalid());
    }
    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if !result.pop() {
                    return Err(invalid());
                }
            }
            Component::CurDir => (),
            _ => result.push(component),
        }
    }
    Ok(result)
}
fn normalized(
    provider: &dyn Provider,
    path: &StreamPath,
    directory: bool,
) -> AppResult<StreamPath> {
    if provider.store_id() != "local" {
        return Ok(path.clone());
    }
    let path = normalize(path.native()?)?;
    if directory {
        return Ok(std::fs::canonicalize(path)?.into());
    }
    let parent = path.parent().ok_or_else(invalid)?.canonicalize()?;
    Ok(parent.join(path.file_name().ok_or_else(invalid)?).into())
}
fn local_identity(provider: &dyn Provider, path: &StreamPath) -> AppResult<Option<String>> {
    if provider.store_id() == "local" {
        // Directory mtime changes when our own transfer adds children. Its stable
        // identity, not that volatile mtime, binds the approved destination.
        return Ok(super::recovery::durability::object_identity(
            path.native()?,
        )?);
    }
    Ok(provider.planning_identity(path)?)
}
fn issue(plan: &mut OperationPlan, severity: &str, code: &str, item: Option<usize>) {
    if plan.issues.len() < 32 {
        plan.issues.push(PlanIssue {
            severity: severity.into(),
            code: code.into(),
            item: item.and_then(|n| n.try_into().ok()),
        });
    }
}
fn digest(plan: &OperationPlan) -> AppResult<String> {
    let mut stable = plan.clone();
    stable.digest.clear();
    // Free space is volatile. Approval binds the required space, while available
    // capacity is rechecked against it, not required to remain bit-for-bit equal.
    stable.limits.available_bytes = None;
    stable.limits.quota_remaining_bytes = None;
    stable
        .issues
        .retain(|i| !matches!(i.code.as_str(), "capacityUnknown" | "noSpace"));
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(&stable).map_err(|_| invalid())?,
    )))
}
impl OperationPlan {
    pub fn ensure_executable(&self) -> AppResult<()> {
        if let Some(issue) = self.issues.iter().find(|i| i.severity == "blocking") {
            let code = match issue.code.as_str() {
                "selfCopy" | "recursiveSource" => code::TRANSFER_INTO_ITSELF,
                "noSpace" => code::FS_NO_SPACE,
                "noStaging" => code::TRANSFER_NO_STAGING,
                _ => code::VALIDATION_BAD_INPUT,
            };
            return Err(AppError::new(code)
                .op("transfer-preflight")
                .with("reason", &issue.code));
        }
        Ok(())
    }
    pub fn matches_review(&self, digest: &str) -> AppResult<()> {
        self.ensure_executable()?;
        if self.digest != digest {
            return Err(invalid().with("reason", "planChanged"));
        }
        Ok(())
    }
}

pub fn build(
    kind: RecoveryKind,
    sources: &[String],
    destination: &str,
    settings: RecoverySettings,
    cancel: &AtomicBool,
) -> AppResult<OperationPlan> {
    if !matches!(kind, RecoveryKind::Copy | RecoveryKind::Move)
        || sources.is_empty()
        || sources.len() > 1000
    {
        return Err(invalid());
    }
    caps::require(destination, Capability::CopyTo)?;
    let (destination_provider, path) = vfs::resolve(destination)?;
    let destination_path = normalized(&*destination_provider, &path, true)?;
    if !destination_provider.is_dir(&destination_path) {
        return Err(invalid());
    }
    let location = RecoveryLocation::capture(destination)?.with_path(&destination_path)?;
    let limits = destination_provider.limits(&destination_path)?;
    let mut plan = OperationPlan {
        version: 1,
        digest: String::new(),
        kind: kind.clone(),
        destination: location,
        destination_generation: vfs::identity::generation(&destination_provider)?,
        destination_identity: local_identity(&*destination_provider, &destination_path)?,
        settings: settings.clone(),
        mappings: Vec::new(),
        limits,
        space: space(&kind, 0, 0, 0, settings.safe, settings.verify)?,
        issues: Vec::new(),
    };
    if settings.safe && !plan.limits.staging {
        issue(&mut plan, "blocking", "noStaging", None);
    }
    if plan.limits.writable == Some(false) {
        issue(&mut plan, "blocking", "readOnly", None);
    }
    if plan.limits.available().is_none() {
        issue(&mut plan, "advisory", "capacityUnknown", None);
    }
    if plan.limits.preserves_metadata != Some(true) {
        issue(&mut plan, "advisory", "metadataUnknown", None);
    }
    if plan.limits.maximum_name_bytes.is_none()
        || plan.limits.maximum_path_bytes.is_none()
        || plan.limits.reserved_names.is_none()
    {
        issue(&mut plan, "advisory", "namesUnknown", None);
    }
    if plan.limits.writable.is_none() {
        issue(&mut plan, "advisory", "writableUnknown", None);
    }
    if destination_provider
        .transport_security()
        .is_some_and(|t| matches!(t.state, crate::net::security::TransportState::Plaintext))
    {
        issue(&mut plan, "decision", "insecure", None);
    }
    let mut walker = walk::Walker::new(cancel);
    let mut names = HashSet::new();
    let mut source_paths: Vec<(String, StreamPath)> = Vec::new();
    let mut source_store: Option<String> = None;
    let mut bytes = 0u64;
    let mut old_bytes = 0u64;
    let mut files = 0u32;
    for (index, raw) in sources.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::new(code::CANCELLED));
        }
        caps::require_pair(
            raw,
            if kind == RecoveryKind::Move {
                Capability::Move
            } else {
                Capability::CopyFrom
            },
            destination,
            Capability::CopyTo,
        )?;
        let (provider, path) = vfs::resolve(raw)?;
        if source_store
            .as_ref()
            .is_some_and(|store| *store != provider.store_id())
        {
            issue(&mut plan, "blocking", "mixedSources", Some(index));
            break;
        }
        source_store = Some(provider.store_id());
        let source = normalized(&*provider, &path, false)?;
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(invalid)?;
        let target = destination_path.join(name)?;
        let same_store = provider.store_id() == destination_provider.store_id();
        if !same_store {
            issue(&mut plan, "decision", "crossProvider", Some(index));
        }
        let containment_source = if provider.store_id() == "local" {
            std::fs::canonicalize(source.native()?)?.into()
        } else {
            source.clone()
        };
        if same_store
            && (containment_source == target || destination_path.starts_with(&containment_source))
        {
            issue(&mut plan, "blocking", "selfCopy", Some(index));
        }
        if source_paths.iter().any(|(store, other)| {
            *store == provider.store_id()
                && (source.starts_with(other) || other.starts_with(&source))
        }) {
            issue(&mut plan, "blocking", "nestedSources", Some(index));
        }
        source_paths.push((provider.store_id(), source.clone()));
        let name_key = if plan.limits.case_sensitive == Some(true) {
            name.to_owned()
        } else {
            walk::folded(name)
        };
        if !names.insert(name_key) {
            issue(&mut plan, "blocking", "duplicateTarget", Some(index));
        }
        let summary = match walker.measure(&*provider, &source, &target, &plan.limits) {
            Ok(summary) => summary,
            Err(error) => {
                if error.kind() == std::io::ErrorKind::Interrupted {
                    return Err(AppError::new(code::CANCELLED));
                }
                let reason = error.to_string();
                let code = match reason.as_str() {
                    "invalidName" | "recursiveSource" | "caseCollision" | "objectLimit"
                    | "overflow" | "sourceChanged" => reason.as_str(),
                    "planningLimit" | "preflight directory limit" => "planningLimit",
                    _ => "unreadable",
                };
                issue(&mut plan, "blocking", code, Some(index));
                break;
            }
        };
        let exists = target
            .native()
            .is_ok_and(|p| std::fs::symlink_metadata(p).is_ok())
            && destination_provider.store_id() == "local"
            || destination_provider.exists(&target);
        let existing_summary = if exists {
            issue(&mut plan, "decision", "collision", Some(index));
            Some(walker.measure(&*destination_provider, &target, &target, &plan.limits)?)
        } else {
            None
        };
        let existing = existing_summary.as_ref().map_or(0, |summary| summary.bytes);
        let native_move = kind == RecoveryKind::Move
            && same_store
            && (provider.store_id() != "local"
                || matches!((source.native(), destination_path.native()), (Ok(s), Ok(d)) if super::recovery::durability::same_device(s, d)));
        if !native_move {
            bytes = checked(bytes.checked_add(summary.bytes))?;
        }
        old_bytes = checked(old_bytes.checked_add(existing))?;
        files = files.checked_add(summary.files).ok_or_else(invalid)?;
        plan.mappings.push(OperationMapping {
            source: RecoveryLocation::capture(raw)?.with_path(&source)?,
            target: plan.destination.with_path(&target)?,
            source_generation: vfs::identity::generation(&provider)?,
            source_version: summary.version,
            target_version: existing_summary.map(|summary| summary.version),
            target_exists: exists,
            files: summary.files,
            bytes: summary.bytes,
            existing_bytes: existing,
            native_move,
        });
    }
    plan.space = space(
        &kind,
        bytes,
        old_bytes,
        files,
        settings.safe,
        settings.verify,
    )?;
    if plan
        .limits
        .available()
        .is_some_and(|free| free < plan.space.additional_bytes)
    {
        issue(&mut plan, "blocking", "noSpace", None);
    }
    if bytes >= 1024 * 1024 * 1024 || files >= 10_000 {
        issue(&mut plan, "decision", "large", None);
    }
    plan.digest = digest(&plan)?;
    Ok(plan)
}

/// Revalidate a plan against the same currently registered provider, capacity and
/// live destination. Used immediately before writes and again before publication.
pub fn revalidate_destination(
    plan: &OperationPlan,
    provider: &dyn Provider,
    path: &StreamPath,
    required: u64,
) -> AppResult<()> {
    let address = plan.destination.resolve()?;
    let (current, _) = vfs::resolve(&address)?;
    if vfs::identity::generation(&current)? != plan.destination_generation
        || (provider.store_id() != "local"
            && !std::ptr::addr_eq(provider as *const dyn Provider, Arc::as_ptr(&current)))
        || !provider.transport_alive()
        || local_identity(provider, path)? != plan.destination_identity
    {
        return Err(invalid().with("reason", "planChanged"));
    }
    let limits = provider.limits(path)?;
    if limits.writable == Some(false) || plan.settings.safe && !limits.staging {
        return Err(invalid().with("reason", "planChanged"));
    }
    if limits.available().is_some_and(|free| free < required) {
        return Err(AppError::new(code::FS_NO_SPACE));
    }
    Ok(())
}

#[tauri::command]
pub async fn transfer_preflight(
    token: String,
    kind: RecoveryKind,
    sources: Vec<String>,
    destination: String,
    settings: RecoverySettings,
) -> AppResult<OperationPlan> {
    if token.is_empty() || token.len() > 128 {
        return Err(invalid());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let cancel = crate::commands::fs::cancel::register("preflight", &token);
        let result = build(kind, &sources, &destination, settings, &cancel);
        crate::commands::fs::cancel::unregister("preflight", &token);
        result
    })
    .await
    .map_err(|_| invalid())?
}
#[tauri::command]
pub fn cancel_preflight(token: String) {
    if token.len() <= 128 {
        crate::commands::fs::cancel::cancel("preflight", &token);
    }
}

pub(crate) fn target_stamp(
    provider: &dyn Provider,
    path: &StreamPath,
) -> std::io::Result<Option<String>> {
    if !provider.exists(path) {
        return Ok(None);
    }
    let cancel = AtomicBool::new(false);
    let mut walker = walk::Walker::new(&cancel);
    Ok(Some(
        walker
            .measure(
                provider,
                path,
                path,
                &crate::vfs::limits::ProviderLimits {
                    case_sensitive: Some(true),
                    ..Default::default()
                },
            )?
            .version,
    ))
}

#[cfg(test)]
pub(crate) fn measure(provider: &dyn Provider, path: &StreamPath) -> std::io::Result<(u32, u64)> {
    let cancel = AtomicBool::new(false);
    let summary = walk::Walker::new(&cancel).measure(
        provider,
        path,
        path,
        &crate::vfs::limits::ProviderLimits {
            case_sensitive: Some(true),
            ..Default::default()
        },
    )?;
    Ok((summary.files, summary.bytes))
}

pub fn revalidate_source(
    mapping: &OperationMapping,
    provider: &dyn Provider,
    limits: &crate::vfs::limits::ProviderLimits,
    cancel: &AtomicBool,
) -> AppResult<()> {
    let raw = mapping.source.resolve()?;
    let (current, source) = vfs::resolve(&raw)?;
    if vfs::identity::generation(&current)? != mapping.source_generation
        || !provider.transport_alive()
        || (provider.store_id() != "local"
            && !std::ptr::addr_eq(provider as *const dyn Provider, Arc::as_ptr(&current)))
    {
        return Err(invalid().with("reason", "planChanged"));
    }
    let target = mapping.target.stream_path()?;
    let measured = walk::Walker::new(cancel).measure(provider, &source, &target, limits)?;
    if measured.version != mapping.source_version {
        return Err(invalid().with("reason", "sourceChanged"));
    }
    Ok(())
}

pub(crate) fn archive_capacity(
    destination: &StreamPath,
    kind: RecoveryKind,
    bytes: u64,
    files: u32,
) -> AppResult<SpaceEstimate> {
    let estimate = space(&kind, bytes, 0, files, true, false)?;
    let limits = vfs::local::LocalProvider.limits(destination)?;
    if limits.writable == Some(false) {
        return Err(AppError::new(code::FS_PERMISSION));
    }
    if limits
        .available()
        .is_some_and(|free| free < estimate.additional_bytes)
    {
        return Err(AppError::new(code::FS_NO_SPACE));
    }
    Ok(estimate)
}
