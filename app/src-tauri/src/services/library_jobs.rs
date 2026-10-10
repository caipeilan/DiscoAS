//! Cancellable fetches and source revisions; no application or window dependency.
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

#[derive(Default, Clone)]
pub struct LibraryJobs(Arc<Mutex<Registry>>);
#[derive(Default)]
struct Registry {
    next: u64,
    active: HashMap<String, Active>,
}
struct Active {
    request_id: String,
    revision: u64,
    cancel: watch::Sender<bool>,
    committing: bool,
}
pub struct LibraryJob {
    jobs: LibraryJobs,
    key: String,
    request_id: String,
    revision: u64,
    receiver: watch::Receiver<bool>,
}
impl LibraryJobs {
    /// Register cancellation before waiting for a protected source snapshot.
    /// A removal can invalidate this job even before the snapshot is read. The token
    /// covers lock waiting, network work and the later commit check; network work
    /// starts only after the snapshot's operation lock has been released.
    pub async fn begin_with_snapshot<T>(
        &self,
        operation: &tokio::sync::Mutex<()>,
        key: String,
        request_id: Option<String>,
        snapshot: impl FnOnce() -> Result<T, String>,
    ) -> Result<(T, LibraryJob), String> {
        let mut job = self.begin(key, request_id)?;
        let guard = job.run(async { Ok(operation.lock().await) }).await?;
        let snapshot = snapshot()?;
        drop(guard);
        Ok((snapshot, job))
    }

    pub fn begin(&self, key: String, request_id: Option<String>) -> Result<LibraryJob, String> {
        let mut registry = self.0.lock().unwrap();
        registry.next = registry.next.wrapping_add(1);
        let revision = registry.next;
        let request_id = request_id.unwrap_or_else(|| format!("background-{revision}"));
        if request_id.is_empty() || request_id.len() > 128 {
            return Err("错误：请求标识无效".into());
        }
        if registry
            .active
            .values()
            .any(|active| active.request_id == request_id)
        {
            return Err("错误：操作正在进行".into());
        }
        if let Some(previous) = registry.active.remove(&key) {
            let _ = previous.cancel.send(true);
        }
        let (cancel, receiver) = watch::channel(false);
        registry.active.insert(
            key.clone(),
            Active {
                request_id: request_id.clone(),
                revision,
                cancel,
                committing: false,
            },
        );
        Ok(LibraryJob {
            jobs: self.clone(),
            key,
            request_id,
            revision,
            receiver,
        })
    }
    pub fn cancel(&self, request_id: &str) -> bool {
        let registry = self.0.lock().unwrap();
        if let Some(active) = registry
            .active
            .values()
            .find(|a| a.request_id == request_id && !a.committing)
        {
            let _ = active.cancel.send(true);
            return true;
        }
        false
    }
    pub fn invalidate_source(&self, key: &str) {
        if let Some(active) = self.0.lock().unwrap().active.remove(key) {
            let _ = active.cancel.send(true);
        }
    }
}
impl LibraryJob {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    pub async fn run<T>(
        &mut self,
        work: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        if *self.receiver.borrow() {
            return Err("操作已取消".into());
        }
        tokio::select! {
            biased;
            _ = self.receiver.changed() => Err("操作已取消".into()),
            result = work => result,
        }
    }
    /// Cancellation is accepted until this transition. The following file commit is synchronous.
    pub fn begin_commit(&self) -> Result<(), String> {
        let mut registry = self.jobs.0.lock().unwrap();
        let active = registry
            .active
            .get_mut(&self.key)
            .filter(|a| a.revision == self.revision)
            .ok_or("操作已取消")?;
        if *active.cancel.borrow() {
            return Err("操作已取消".into());
        }
        active.committing = true;
        Ok(())
    }
}
impl Drop for LibraryJob {
    fn drop(&mut self) {
        let mut registry = self.jobs.0.lock().unwrap();
        if registry
            .active
            .get(&self.key)
            .is_some_and(|a| a.revision == self.revision)
        {
            registry.active.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_drops_network_work_and_cannot_cancel_committed_files() {
        let jobs = LibraryJobs::default();
        let mut job = jobs
            .begin("source".into(), Some("发现 / 1: retry".into()))
            .unwrap();
        assert!(jobs.cancel("发现 / 1: retry"));
        assert_eq!(
            job.run(std::future::pending::<Result<(), String>>())
                .await
                .unwrap_err(),
            "操作已取消"
        );
        assert!(job.begin_commit().is_err());
        drop(job);
        let job = jobs.begin("source".into(), None).unwrap();
        job.begin_commit().unwrap();
        assert!(!jobs.cancel(job.request_id()));
    }
    #[tokio::test]
    async fn later_source_request_and_removal_prevent_stale_commit() {
        let jobs = LibraryJobs::default();
        let first = jobs.begin("source".into(), None).unwrap();
        let second = jobs.begin("source".into(), None).unwrap();
        assert!(first.begin_commit().is_err());
        drop(first);
        assert!(jobs.cancel(second.request_id()));
        let third = jobs.begin("other".into(), None).unwrap();
        jobs.invalidate_source("other");
        assert!(third.begin_commit().is_err());
    }

    #[tokio::test]
    async fn snapshot_registration_blocks_removal_and_reimport_cannot_make_an_old_job_current() {
        let jobs = LibraryJobs::default();
        let operation = Arc::new(tokio::sync::Mutex::new(()));
        let source = Arc::new(Mutex::new("old import".to_string()));
        let (snapshot_read, snapshot_ready) = tokio::sync::oneshot::channel();
        let (resume, resumed) = std::sync::mpsc::channel();
        let task_jobs = jobs.clone();
        let task_operation = operation.clone();
        let task_source = source.clone();
        let task = tokio::task::spawn_blocking(move || {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(task_jobs.begin_with_snapshot(
                    &task_operation,
                    "source".into(),
                    None,
                    || {
                        let snapshot = task_source.lock().unwrap().clone();
                        snapshot_read.send(()).unwrap();
                        resumed.recv().unwrap();
                        Ok(snapshot)
                    },
                ))
                .unwrap()
        });
        snapshot_ready.await.unwrap();
        // Pause during snapshot loading on a blocking worker, leaving this runtime
        // free to inspect the lock. The token is already registered at this point.
        let protected = operation.try_lock().is_err();
        resume.send(()).unwrap();
        let (snapshot, old_job) = task.await.unwrap();
        assert!(protected);
        assert_eq!(snapshot, "old import");
        let guard = operation.lock().await;
        source.lock().unwrap().clear();
        jobs.invalidate_source("source");
        *source.lock().unwrap() = "new import".into();
        drop(guard);
        let (snapshot, new_job) = jobs
            .begin_with_snapshot(&operation, "source".into(), None, || {
                Ok(source.lock().unwrap().clone())
            })
            .await
            .unwrap();
        assert_eq!(snapshot, "new import");
        assert_eq!(old_job.begin_commit().unwrap_err(), "操作已取消");
        assert!(new_job.begin_commit().is_ok());
        assert!(
            operation.try_lock().is_ok(),
            "Network work must not keep the operation lock"
        );
    }

    #[tokio::test]
    async fn queued_snapshot_requests_can_be_cancelled_or_invalidated_before_reading_state() {
        for remove_source in [false, true] {
            let jobs = LibraryJobs::default();
            let operation = tokio::sync::Mutex::new(());
            let guard = operation.lock().await;
            let snapshot_read = std::sync::atomic::AtomicBool::new(false);
            let mut waiting = Box::pin(jobs.begin_with_snapshot(
                &operation,
                "source".into(),
                Some("waiting-request".into()),
                || {
                    snapshot_read.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                },
            ));
            // Poll registration once while the operation is locked, without sleeps
            // or another task whose scheduling would make the assertion unreliable.
            tokio::select! {
                biased;
                _ = &mut waiting => panic!("The held operation lock must queue this request"),
                _ = std::future::ready(()) => {},
            }
            if remove_source {
                jobs.invalidate_source("source");
            } else {
                assert!(jobs.cancel("waiting-request"));
            }
            assert_eq!(waiting.await.err().as_deref(), Some("操作已取消"));
            assert!(!snapshot_read.load(std::sync::atomic::Ordering::SeqCst));
            assert!(jobs.0.lock().unwrap().active.is_empty());
            // Cancellation completed before the unrelated operation released its lock.
            assert!(operation.try_lock().is_err());
            drop(guard);
        }
    }
}
