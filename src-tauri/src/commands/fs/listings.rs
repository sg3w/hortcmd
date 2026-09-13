//! Only concurrent directory reads are shared. Results are never retained as a
//! cache, so an explicit refresh after a write always starts a fresh read.
use super::dir::DirListing;
use crate::errors::{code, AppError, AppResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
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
    pub async fn get(
        self: &Arc<Self>,
        key: String,
        read: impl FnOnce() -> AppResult<DirListing> + Send + 'static,
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
                        let result = tokio::task::spawn_blocking(move || {
                            let _permit = permit;
                            read().map(Arc::new)
                        })
                        .await
                        .unwrap_or_else(|_| Err(unavailable("directory reader stopped")));
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
    static READERS: OnceLock<Arc<Listings>> = OnceLock::new();
    static REQUESTS: Semaphore = Semaphore::const_new(64);
    let _request = REQUESTS
        .try_acquire()
        .map_err(|_| unavailable("too many directory requests"))?;
    let original = path.clone();
    let (provider, resolved, key, remote) = tokio::task::spawn_blocking(move || {
        let (provider, resolved) = crate::vfs::resolve(&path)?;
        let remote = crate::vfs::parse_address(&path)?.scheme != crate::vfs::Scheme::Local;
        let canonical = if !remote {
            std::fs::canonicalize(&resolved)?
                .to_string_lossy()
                .into_owned()
        } else {
            // Remote symlinks make lexical '..' collapsing unsafe. Preserve
            // provider paths while unifying equivalent connection authorities.
            resolved.clone()
        };
        let identity = if remote {
            format!("{:p}", Arc::as_ptr(&provider))
        } else {
            String::new()
        };
        let key = format!("{}\0{identity}\0{canonical}", provider.store_id());
        Ok::<_, AppError>((provider, resolved, key, remote))
    })
    .await
    .map_err(|_| unavailable("directory resolution stopped"))??;
    let result = READERS
        .get_or_init(|| Arc::new(Listings::default()))
        .get(key, move || {
            let result = provider.list(&resolved)?;
            if remote {
                let (current, _) = crate::vfs::resolve(&original)?;
                if !Arc::ptr_eq(&current, &provider) {
                    return Err(unavailable("connection changed during directory read"));
                }
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
}
