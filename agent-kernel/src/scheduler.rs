//! Cooperative scheduler facade for agent workloads.

use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

/// Maximum number of concurrent tasks allowed per agent.
#[derive(Debug, Clone, Copy)]
pub struct SchedulerConfig {
    max_concurrency: NonZeroUsize,
}

impl SchedulerConfig {
    /// Creates a new configuration with the supplied concurrency limit.
    #[must_use]
    pub const fn new(max_concurrency: NonZeroUsize) -> Self {
        Self { max_concurrency }
    }

    /// Returns the configured concurrency limit.
    #[must_use]
    pub const fn max_concurrency(self) -> NonZeroUsize {
        self.max_concurrency
    }
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self::new(NonZeroUsize::new(32).expect("non-zero"))
    }
}

/// Lightweight wrapper around `tokio::spawn` that enforces per-agent concurrency.
#[derive(Debug, Clone)]
pub struct TaskScheduler {
    semaphore: Arc<Semaphore>,
    closed: Arc<AtomicBool>,
    config: SchedulerConfig,
}

impl TaskScheduler {
    /// Constructs a scheduler using the provided configuration.
    #[must_use]
    pub fn new(config: SchedulerConfig) -> Self {
        let permits = config.max_concurrency().get();
        Self {
            semaphore: Arc::new(Semaphore::new(permits)),
            closed: Arc::new(AtomicBool::new(false)),
            config,
        }
    }

    /// Returns the associated configuration.
    #[must_use]
    pub const fn config(&self) -> SchedulerConfig {
        self.config
    }

    /// Returns `true` if the scheduler has been closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Closes the scheduler, preventing new tasks from being spawned.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.semaphore.close();
    }

    /// Spawns a future, respecting the configured concurrency limit.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError::Closed`] when the scheduler is closed before the
    /// task is enqueued.
    ///
    /// # Panics
    ///
    /// Panics if the scheduler is closed while a task is awaiting a concurrency
    /// permit. This indicates that `close` was invoked concurrently with task
    /// submission.
    pub fn spawn<F, T>(&self, future: F) -> SchedulerResult<JoinHandle<T>>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        if self.is_closed() {
            return Err(SchedulerError::Closed);
        }

        let semaphore = Arc::clone(&self.semaphore);

        let handle = tokio::spawn(async move {
            let permit = semaphore
                .acquire_owned()
                .await
                .expect("scheduler closed while awaiting permit");
            let output = future.await;
            drop(permit);
            output
        });

        Ok(handle)
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new(SchedulerConfig::default())
    }
}

/// Errors produced by the scheduler.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SchedulerError {
    /// Scheduler is closed and will not accept new tasks.
    #[error("scheduler closed")]
    Closed,
}

/// Result alias for scheduler operations.
pub type SchedulerResult<T> = Result<T, SchedulerError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn respects_max_concurrency() {
        let config = SchedulerConfig::new(NonZeroUsize::new(2).unwrap());
        let scheduler = TaskScheduler::new(config);
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..3 {
            let scheduler = scheduler.clone();
            let in_flight = Arc::clone(&in_flight);
            let max_seen = Arc::clone(&max_seen);
            handles.push(
                scheduler
                    .spawn(async move {
                        let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        max_seen.fetch_max(current, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        in_flight.fetch_sub(1, Ordering::SeqCst);
                    })
                    .unwrap(),
            );
        }

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(max_seen.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn close_prevents_new_tasks() {
        let scheduler = TaskScheduler::default();
        scheduler.close();

        let result = scheduler.spawn(async move {});
        assert_eq!(result.unwrap_err(), SchedulerError::Closed);
    }
}
