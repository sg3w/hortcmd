use super::*;
use std::sync::atomic::Ordering;

fn matches(
    row: &Row,
    path: &str,
    query: &IndexQuery,
    name: &regex::Regex,
    path_pattern: &regex::Regex,
) -> bool {
    name.is_match(&row.name)
        && path_pattern.is_match(path)
        && match query.kind.as_str() {
            "file" => !row.directory,
            "directory" => row.directory,
            _ => true,
        }
        && query.min_size.is_none_or(|size| row.size >= size)
        && query.max_size.is_none_or(|size| row.size <= size)
        && query.modified_after.is_none_or(|time| row.modified >= time)
        && (query.content.is_empty()
            || row.content.as_ref().is_some_and(|text| {
                if query.case_sensitive {
                    text.contains(&query.content)
                } else {
                    text.to_lowercase().contains(&query.content.to_lowercase())
                }
            }))
}
fn page(
    snapshot: &Snapshot,
    query: &IndexQuery,
    freshness: &str,
    source: &str,
    address: Option<&str>,
    cancel: &AtomicBool,
) -> AppResult<IndexPage> {
    if cancel.load(Ordering::Relaxed) {
        return Err(AppError::new(code::CANCELLED));
    }
    if query.after.is_some() && query.revision.is_none() {
        return Err(bad("pagination requires an index revision"));
    }
    if query.limit == 0
        || query.limit > 500
        || query.content.len() > 1024
        || !["any", "file", "directory"].contains(&query.kind.as_str())
    {
        return Err(bad("invalid index query"));
    }
    if !query.content.is_empty() && !snapshot.policy.content {
        return Err(bad("content indexing was not approved for this scope"));
    }
    if query
        .revision
        .is_some_and(|revision| revision != snapshot.revision)
    {
        return Err(bad("index changed; restart pagination"));
    }
    let name = glob(
        if query.name.is_empty() {
            "*"
        } else {
            &query.name
        },
        query.case_sensitive,
    )?;
    let path_pattern = glob(
        if query.path.is_empty() {
            "*"
        } else {
            &query.path
        },
        query.case_sensitive,
    )?;
    let mut hits = Vec::new();
    let mut next = None;
    for (path, row) in &snapshot.rows {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::new(code::CANCELLED));
        }
        if query.after.as_ref().is_some_and(|after| path <= after)
            || !matches(row, path, query, &name, &path_pattern)
        {
            continue;
        }
        if hits.len() == query.limit as usize {
            next = hits.last().map(|hit: &IndexHit| hit.relative.clone());
            break;
        }
        hits.push(IndexHit {
            id: hex::encode(Sha256::digest(format!("{}\0{path}", snapshot.id))),
            relative: path.clone(),
            address: address.map(|root| {
                let suffix = if root.contains("://") {
                    path.split('/')
                        .map(crate::vfs::location::encode)
                        .collect::<Vec<_>>()
                        .join("/")
                } else {
                    path.to_owned()
                };
                format!("{}/{suffix}", root.trim_end_matches('/'))
            }),
            directory: row.directory,
            size: row.size,
            modified: row.modified,
            provenance: if query.content.is_empty() {
                "metadata"
            } else {
                "content"
            }
            .into(),
        });
    }
    Ok(IndexPage {
        hits,
        next,
        revision: snapshot.revision,
        freshness: freshness.into(),
        source: source.into(),
        fallback: query.adapter == "native",
    })
}
#[tauri::command]
pub async fn index_query(id: String, query: IndexQuery, token: String) -> AppResult<IndexPage> {
    static QUERIES: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    let permit = QUERIES
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| bad("too many index queries"))?;
    if !["application", "live", "native"].contains(&query.adapter.as_str()) {
        return Err(bad("unknown index adapter"));
    }
    let cancel = super::super::cancel::register("index", &token);
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _crawl = if query.adapter == "live" {
            Some(
                worker()
                    .try_acquire_owned()
                    .map_err(|_| bad("index worker busy"))?,
            )
        } else {
            None
        };
        let scope = get(&id)?;
        let mut scope = scope.lock().unwrap_or_else(|e| e.into_inner());
        let current_generation = generation(&scope.snapshot.location);
        let address = current_generation
            .as_ref()
            .ok()
            .and_then(|_| scope.snapshot.location.resolve().ok());
        if address.is_none() && !scope.snapshot.policy.offline {
            return Err(bad("reconnect to query this scope"));
        }
        let live = query.adapter == "live";
        if live {
            if address.is_none() {
                return Err(bad("reconnect for live search"));
            }
            if resource_pressure(scope.snapshot.policy.allow_battery) {
                return Err(bad("search paused for battery or memory pressure"));
            }
            if matches!(scope.snapshot.location, RecoveryLocation::Local { .. })
                && current_generation
                    .as_ref()
                    .is_ok_and(|current| current != &scope.snapshot.generation)
            {
                return Err(bad("index root changed; approve again"));
            }
            scope.dirty.store(false, Ordering::Relaxed);
            let mut scratch = scope.snapshot.clone();
            scratch.clear_rows();
            scratch.reset_queue(VecDeque::from([String::new()]));
            scratch.cycle += 1;
            scratch.revision += 1;
            scratch.limited = false;
            scratch.unreadable = 0;
            scratch.generation = generation(&scratch.location)?;
            while !scratch.queue.is_empty() && !scratch.limited {
                if cancel.load(Ordering::Relaxed) {
                    return Err(AppError::new(code::CANCELLED));
                }
                crawler::step(&mut scratch, &cancel)?;
            }
            if scratch.limited {
                return Err(bad("live scan reached the scope limit"));
            }
            scope.snapshot = scratch;
            scope.save()?;
        }
        let latest_generation = generation(&scope.snapshot.location);
        let address = latest_generation
            .as_ref()
            .ok()
            .and_then(|_| scope.snapshot.location.resolve().ok());
        if address.is_none() && !scope.snapshot.policy.offline {
            return Err(bad("scope disconnected during query"));
        }
        let changed = latest_generation.is_ok_and(|current| current != scope.snapshot.generation);
        let freshness = if address.is_none() {
            "offline"
        } else if scope.snapshot.limited || !scope.snapshot.queue.is_empty() {
            "incomplete"
        } else if changed || scope.stale() {
            "stale"
        } else {
            "snapshot"
        };
        page(
            &scope.snapshot,
            &query,
            freshness,
            if live {
                "live_scan"
            } else {
                "application_index"
            },
            address.as_deref(),
            &cancel,
        )
    })
    .await
    .map_err(|_| bad("index query worker stopped"));
    super::super::cancel::unregister("index", &token);
    result?
}
#[cfg(test)]
pub(super) fn test_page(snapshot: &Snapshot, query: &IndexQuery) -> AppResult<IndexPage> {
    page(
        snapshot,
        query,
        "snapshot",
        "application_index",
        Some("/root"),
        &AtomicBool::new(false),
    )
}
