//! Concurrent reads share a worker; a bounded presentation cache reuses fresh results.
//! Explicit refresh and known mutations invalidate both stored and in-flight data.
use super::{cache, dir::DirListing};
use crate::errors::{code, AppError, AppResult};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::sync::{watch, Semaphore};

type Outcome = AppResult<Arc<DirListing>>;
type Pending = HashMap<String, watch::Sender<Option<Outcome>>>;

pub struct Listings {
    pending: Mutex<Pending>,
    workers: Arc<Semaphore>,
}

impl Default for Listings {
    fn default() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            workers: Arc::new(Semaphore::new(4)),
        }
    }
}

fn unavailable(detail: &str) -> AppError {
    AppError::new(code::FS_IO).op("list-dir").detail(detail)
}

impl Listings {
    #[cfg(test)]
    pub async fn get(
        self: &Arc<Self>,
        key: String,
        read: impl FnOnce() -> AppResult<DirListing> + Send + 'static,
    ) -> Outcome {
        self.get_cancellable(key, move |_| read()).await
    }
    pub async fn get_cancellable(
        self: &Arc<Self>,
        key: String,
        read: impl FnOnce(Arc<AtomicBool>) -> AppResult<DirListing> + Send + 'static,
    ) -> Outcome {
        let mut receiver = {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(sender) = pending.get(&key) {
                sender.subscribe()
            } else {
                if pending.len() >= 32 {
                    return Err(unavailable(
                        "too many directory reads; retry after pending reads finish",
                    ));
                }
                let (sender, receiver) = watch::channel(None);
                pending.insert(key.clone(), sender.clone());
                let owner = self.clone();
                tokio::spawn(async move {
                    // No reader remains: do not start queued filesystem work.
                    let permit = tokio::select! {
                        _ = sender.closed() => None,
                        permit = owner.workers.clone().acquire_owned() => permit.ok(),
                    };
                    let result = if let Some(permit) = permit {
                        let cancel = Arc::new(AtomicBool::new(false));
                        let flag = cancel.clone();
                        let mut worker = tokio::task::spawn_blocking(move || {
                            let _permit = permit;
                            read(flag).map(Arc::new)
                        });
                        let joined = tokio::select! {
                            result = &mut worker => result,
                            _ = sender.closed() => { cancel.store(true, Ordering::Relaxed); worker.await }
                        };
                        let result =
                            joined.unwrap_or_else(|_| Err(unavailable("directory reader stopped")));
                        Some(result)
                    } else {
                        None
                    };
                    let mut pending = owner.pending.lock().unwrap_or_else(|e| e.into_inner());
                    pending.remove(&key);
                    // Removal and publication share the lock: a fresh caller
                    // can never subscribe to an already completed read.
                    if let Some(result) = result {
                        sender.send_replace(Some(result));
                    }
                });
                receiver
            }
        };
        loop {
            if let Some(result) = receiver.borrow_and_update().clone() {
                return result;
            }
            if receiver.changed().await.is_err() {
                return Err(unavailable("directory read cancelled"));
            }
        }
    }
}

pub async fn read(path: String) -> AppResult<DirListing> {
    read_with(path, false).await
}
pub async fn read_fresh(path: String) -> AppResult<DirListing> {
    read_with(path, true).await
}
async fn read_with(path: String, fresh: bool) -> AppResult<DirListing> {
    static READERS: OnceLock<Arc<Listings>> = OnceLock::new();
    static REQUESTS: Semaphore = Semaphore::const_new(64);
    let _request = REQUESTS
        .try_acquire()
        .map_err(|_| unavailable("too many directory requests"))?;
    let original = path.clone();
    let (provider, resolved, key, remote) = tokio::task::spawn_blocking(move || {
        let (provider, resolved) = crate::vfs::resolve(&path)?;
        let remote = crate::vfs::parse_address(&path)?.scheme() != crate::vfs::Scheme::Local;
        let canonical = if !remote {
            std::fs::canonicalize(resolved.native()?)?
                .to_string_lossy()
                .into_owned()
        } else {
            // Remote symlinks make lexical '..' collapsing unsafe. Preserve
            // provider paths while unifying equivalent connection authorities.
            resolved.to_string_lossy().into_owned()
        };
        let identity = if remote {
            crate::vfs::identity::generation(&provider)?
        } else {
            format!(
                "local:{}",
                provider
                    .planning_identity(&crate::vfs::StreamPath::from(&canonical))?
                    .unwrap_or_default()
            )
        };
        let capabilities = crate::vfs::caps::for_address(&path)?;
        let options = hex::encode(Sha256::digest(
            serde_json::to_vec(&capabilities).map_err(|_| unavailable("capability identity"))?,
        ));
        let key = cache::Key {
            store: hex::encode(Sha256::digest(provider.store_id().as_bytes())),
            identity,
            path: canonical,
            options,
        };
        Ok::<_, AppError>((provider, resolved, key, remote))
    })
    .await
    .map_err(|_| unavailable("directory resolution stopped"))??;
    if fresh {
        cache::invalidate();
    }
    let ticket = {
        let mut cached = cache::listings().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(result) = cached.get(&key, Instant::now()) {
            return Ok((*result).clone());
        }
        cached.ticket()
    };
    let pending_key = format!(
        "{}\0{}\0{}\0{}\0{ticket}",
        key.store, key.identity, key.path, key.options
    );
    let result = READERS
        .get_or_init(|| Arc::new(Listings::default()))
        .get_cancellable(pending_key, move |cancel| {
            let result = provider.list_cancellable(&resolved.to_string_lossy(), &cancel)?;
            if remote {
                let (current, _) = crate::vfs::resolve(&original)?;
                if !Arc::ptr_eq(&current, &provider) {
                    return Err(unavailable("connection changed during directory read"));
                }
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::new(code::CANCELLED));
            }
            let weight = result
                .path
                .len()
                .saturating_add(key.path.len())
                .saturating_add(256)
                .saturating_add(
                    result
                        .entries
                        .iter()
                        .map(|e| {
                            e.name
                                .len()
                                .saturating_add(std::mem::size_of::<super::dir::DirEntry>())
                        })
                        .sum::<usize>(),
                );
            let mut cached = cache::listings().lock().unwrap_or_else(|e| e.into_inner());
            let value = Arc::new(result.clone());
            // Oversized directories remain live reads; the cache never retains them.
            let weight = if result.entries.len() > 50_000 {
                usize::MAX
            } else {
                weight
            };
            if !cached.put(key, value, weight, ticket, Instant::now()) {
                return Err(unavailable("directory invalidated during read"));
            }
            Ok(result)
        })
        .await?;
    Ok((*result).clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn concurrent_consumers_share_one_read_but_the_next_refresh_is_fresh() {
        let listings = Arc::new(Listings::default());
        let reads = Arc::new(AtomicUsize::new(0));
        let (finish, wait) = std::sync::mpsc::channel();
        let first = {
            let listings = listings.clone();
            let reads = reads.clone();
            tokio::spawn(async move {
                listings
                    .get("same".into(), move || {
                        reads.fetch_add(1, Ordering::SeqCst);
                        wait.recv().unwrap();
                        Ok(DirListing {
                            path: "/same".into(),
                            entries: vec![],
                        })
                    })
                    .await
            })
        };
        while reads.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        let mut second =
            Box::pin(listings.get("same".into(), || panic!("duplicate filesystem read")));
        // Poll the second consumer until it has subscribed, without sleeps.
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(
                second.as_mut().poll(cx).is_pending()
            ))
            .await
        );
        finish.send(()).unwrap();
        assert!(Arc::ptr_eq(
            &first.await.unwrap().unwrap(),
            &second.await.unwrap()
        ));
        assert!(listings.pending.lock().unwrap().is_empty());
        let next = listings
            .get("same".into(), || {
                Ok(DirListing {
                    path: "/fresh".into(),
                    entries: vec![],
                })
            })
            .await
            .unwrap();
        assert_eq!(next.path, "/fresh");
    }

    #[tokio::test]
    async fn a_full_queue_refuses_new_locations_and_dropped_waiters_release_it() {
        let listings = Arc::new(Listings::default());
        let _workers = listings.workers.acquire_many(4).await.unwrap();
        let mut requests = Vec::new();
        for index in 0..32 {
            let mut request =
                Box::pin(listings.get(index.to_string(), || panic!("queued work ran")));
            assert!(
                std::future::poll_fn(|cx| std::task::Poll::Ready(
                    request.as_mut().poll(cx).is_pending()
                ))
                .await
            );
            requests.push(request);
        }
        assert!(listings
            .get("overflow".into(), || panic!("overflow work ran"))
            .await
            .is_err());
        assert_eq!(listings.pending.lock().unwrap().len(), 32);
        drop(requests);
        while !listings.pending.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn abandoned_queued_reads_release_their_slot_without_running() {
        let listings = Arc::new(Listings::default());
        let _all_workers = listings.workers.acquire_many(4).await.unwrap();
        let mut request = Box::pin(listings.get("unused".into(), || panic!("abandoned work ran")));
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(
                request.as_mut().poll(cx).is_pending()
            ))
            .await
        );
        drop(request);
        while !listings.pending.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    }
    #[tokio::test]
    async fn running_work_is_cancelled_only_after_its_last_consumer_leaves() {
        let listings = Arc::new(Listings::default());
        let (started, flag) = tokio::sync::oneshot::channel();
        let mut first = Box::pin(listings.get_cancellable("shared".into(), move |cancel| {
            started.send(cancel.clone()).unwrap();
            while !cancel.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            Err(AppError::new(code::CANCELLED))
        }));
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(first.as_mut().poll(cx).is_pending()))
                .await
        );
        let cancel = flag.await.unwrap();
        let mut second = Box::pin(listings.get("shared".into(), || panic!("duplicate")));
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(
                second.as_mut().poll(cx).is_pending()
            ))
            .await
        );
        drop(first);
        tokio::task::yield_now().await;
        assert!(!cancel.load(Ordering::Relaxed));
        drop(second);
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !listings.pending.lock().unwrap().is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn warm_reads_and_explicit_refresh_use_real_local_provider() {
        const CHILD: &str = "HORTCMD_CACHE_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "commands::fs::listings::tests::warm_reads_and_explicit_refresh_use_real_local_provider", "--nocapture"])
                .env(CHILD, "1").output().unwrap();
            assert!(
                result.status.success(),
                "{} {}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let dir = std::env::temp_dir().join(format!("hortcmd-cache-{}", crate::net::unique_id()));
        std::fs::create_dir(&dir).unwrap();
        for index in 0..200 {
            std::fs::write(dir.as_path().join(format!("entry-{index}")), "data").unwrap();
        }
        let path = dir.as_path().to_string_lossy().into_owned();
        cache::invalidate();
        let first = read(path.clone()).await.unwrap();
        let before = cache::cache_metrics().listings;
        for _ in 0..10 {
            let next = read(path.clone()).await.unwrap();
            assert_eq!(next.entries.len(), first.entries.len());
        }
        let after = cache::cache_metrics().listings;
        assert_eq!(after.hits - before.hits, 10);
        assert_eq!(after.misses, before.misses);
        std::fs::write(dir.as_path().join("new"), "new").unwrap();
        assert_eq!(read_fresh(path.clone()).await.unwrap().entries.len(), 201);
        {
            let _mutation = cache::Mutation::begin();
            std::fs::remove_file(dir.as_path().join("new")).unwrap();
            assert_eq!(read(path.clone()).await.unwrap().entries.len(), 200);
            assert_eq!(cache::cache_metrics().listings.entries, 0);
        }
        assert_eq!(cache::cache_metrics().listings.entries, 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
