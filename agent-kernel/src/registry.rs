//! Agent registry integration for Relay mesh discovery and heartbeats.

use std::fmt;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use agent_primitives::AgentManifest;
use async_trait::async_trait;
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio::time::{sleep, MissedTickBehavior};
use tracing::{info, warn};

use crate::{AgentState, SchedulerError, TaskScheduler};

/// Configuration for registration and heartbeat maintenance.
#[derive(Debug, Clone, Copy)]
pub struct RegistrationConfig {
    heartbeat_interval: Duration,
    initial_retry_delay: Duration,
    max_retry_delay: Duration,
    max_consecutive_failures: NonZeroUsize,
}

impl RegistrationConfig {
    /// Creates a new configuration.
    #[must_use]
    pub fn new(
        heartbeat_interval: Duration,
        initial_retry_delay: Duration,
        max_retry_delay: Duration,
        max_consecutive_failures: NonZeroUsize,
    ) -> Self {
        Self {
            heartbeat_interval,
            initial_retry_delay,
            max_retry_delay,
            max_consecutive_failures,
        }
    }

    /// Returns the heartbeat interval.
    #[must_use]
    pub const fn heartbeat_interval(self) -> Duration {
        self.heartbeat_interval
    }

    /// Returns the initial retry delay.
    #[must_use]
    pub const fn initial_retry_delay(self) -> Duration {
        self.initial_retry_delay
    }

    /// Returns the maximum retry delay.
    #[must_use]
    pub const fn max_retry_delay(self) -> Duration {
        self.max_retry_delay
    }

    /// Returns the limit on consecutive heartbeat failures before re-registration.
    #[must_use]
    pub const fn max_consecutive_failures(self) -> NonZeroUsize {
        self.max_consecutive_failures
    }

    /// Validates the configuration.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::InvalidConfig`] when any duration is zero or the
    /// retry delay bounds are inconsistent.
    pub fn validate(self) -> RegistryResult<()> {
        if self.heartbeat_interval.is_zero() {
            return Err(RegistryError::InvalidConfig(
                "heartbeat interval must be greater than zero",
            ));
        }
        if self.initial_retry_delay.is_zero() {
            return Err(RegistryError::InvalidConfig(
                "initial retry delay must be greater than zero",
            ));
        }
        if self.max_retry_delay.is_zero() {
            return Err(RegistryError::InvalidConfig(
                "max retry delay must be greater than zero",
            ));
        }
        if self.initial_retry_delay > self.max_retry_delay {
            return Err(RegistryError::InvalidConfig(
                "initial retry delay cannot exceed max retry delay",
            ));
        }
        Ok(())
    }
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(10),
            initial_retry_delay: Duration::from_secs(1),
            max_retry_delay: Duration::from_secs(30),
            max_consecutive_failures: NonZeroUsize::new(3).expect("non-zero"),
        }
    }
}

/// Result alias for registry operations.
pub type RegistryResult<T> = Result<T, RegistryError>;

/// Errors surfaced by registry integration.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Registration configuration was invalid.
    #[error("invalid registration configuration: {0}")]
    InvalidConfig(&'static str),
    /// Scheduler rejected a task submission.
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    /// Registry backend failure.
    #[error("registry backend error: {reason}")]
    Backend {
        /// Human-readable context provided by the backend.
        reason: String,
    },
}

impl RegistryError {
    /// Convenience helper to construct backend errors.
    #[must_use]
    pub fn backend(reason: impl Into<String>) -> Self {
        Self::Backend {
            reason: reason.into(),
        }
    }
}

/// Trait implemented by discovery/registry backends.
#[async_trait]
pub trait AgentRegistry: Send + Sync {
    /// Registers an agent manifest with the mesh.
    async fn register(&self, manifest: &AgentManifest) -> RegistryResult<()>;

    /// Sends a heartbeat for an already registered agent.
    async fn heartbeat(&self, manifest: &AgentManifest) -> RegistryResult<()>;

    /// Removes the agent from the registry.
    async fn deregister(&self, manifest: &AgentManifest) -> RegistryResult<()>;
}

pub(crate) struct RegistrationController {
    registry: Arc<dyn AgentRegistry>,
    manifest: Arc<AgentManifest>,
    config: RegistrationConfig,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl fmt::Debug for RegistrationController {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegistrationController")
            .field("registry", &"dyn AgentRegistry")
            .field("manifest", &self.manifest.id())
            .field("config", &self.config)
            .field("shutdown", &self.shutdown.load(Ordering::Relaxed))
            .field("worker", &self.worker.is_some())
            .finish()
    }
}

impl RegistrationController {
    pub(crate) fn new(
        registry: Arc<dyn AgentRegistry>,
        manifest: AgentManifest,
        config: RegistrationConfig,
    ) -> Self {
        Self {
            registry,
            manifest: Arc::new(manifest),
            config,
            shutdown: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }

    pub(crate) fn on_state_change(
        &mut self,
        state: AgentState,
        scheduler: &TaskScheduler,
    ) -> RegistryResult<()> {
        match state {
            AgentState::Ready | AgentState::Active => {
                self.ensure_worker(scheduler)?;
            }
            AgentState::Retiring | AgentState::Terminated => {
                self.shutdown.store(true, Ordering::Release);
                self.spawn_deregister(scheduler)?;
                if let Some(handle) = self.worker.take() {
                    handle.abort();
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn ensure_worker(&mut self, scheduler: &TaskScheduler) -> RegistryResult<()> {
        if self.worker.is_some() {
            return Ok(());
        }

        self.config.validate()?;

        let registry = Arc::clone(&self.registry);
        let manifest = Arc::clone(&self.manifest);
        let shutdown = Arc::clone(&self.shutdown);
        let config = self.config;

        let handle = scheduler.spawn(async move {
            run_registration_loop(registry, manifest, shutdown, config).await;
        })?;

        self.worker = Some(handle);
        Ok(())
    }

    fn spawn_deregister(&self, scheduler: &TaskScheduler) -> RegistryResult<()> {
        let registry = Arc::clone(&self.registry);
        let manifest = Arc::clone(&self.manifest);
        scheduler.spawn(async move {
            if let Err(err) = registry.deregister(&manifest).await {
                warn!(?err, "agent deregistration failed");
            } else {
                info!(agent_id = %manifest.id(), "agent deregistered");
            }
        })?;
        Ok(())
    }
}

async fn run_registration_loop(
    registry: Arc<dyn AgentRegistry>,
    manifest: Arc<AgentManifest>,
    shutdown: Arc<AtomicBool>,
    config: RegistrationConfig,
) {
    let mut retry_delay = config.initial_retry_delay();

    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }

        match registry.register(&manifest).await {
            Ok(()) => {
                info!(agent_id = %manifest.id(), "agent registered with mesh");
                retry_delay = config.initial_retry_delay();
                if !run_heartbeat_loop(
                    Arc::clone(&registry),
                    Arc::clone(&manifest),
                    Arc::clone(&shutdown),
                    config,
                )
                .await
                {
                    continue;
                }
                break;
            }
            Err(err) => {
                warn!(?err, "agent registration failed; retrying");
                sleep(retry_delay).await;
                retry_delay = (retry_delay * 2).min(config.max_retry_delay());
            }
        }
    }
}

async fn run_heartbeat_loop(
    registry: Arc<dyn AgentRegistry>,
    manifest: Arc<AgentManifest>,
    shutdown: Arc<AtomicBool>,
    config: RegistrationConfig,
) -> bool {
    let mut failures: usize = 0;
    let mut interval = tokio::time::interval(config.heartbeat_interval());
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    while !shutdown.load(Ordering::Acquire) {
        interval.tick().await;
        if shutdown.load(Ordering::Acquire) {
            break;
        }

        match registry.heartbeat(&manifest).await {
            Ok(()) => {
                failures = 0;
            }
            Err(err) => {
                failures += 1;
                warn!(?err, failures, "heartbeat failure");
                if failures >= config.max_consecutive_failures().get() {
                    warn!(
                        failures,
                        "heartbeat failure threshold reached; attempting re-registration"
                    );
                    return false;
                }
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    use agent_primitives::{AgentId, Capability, CapabilityId};

    struct MockRegistry {
        registers: Arc<AtomicUsize>,
        heartbeats: Arc<AtomicUsize>,
        deregistrations: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AgentRegistry for MockRegistry {
        async fn register(&self, _manifest: &AgentManifest) -> RegistryResult<()> {
            self.registers.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn heartbeat(&self, _manifest: &AgentManifest) -> RegistryResult<()> {
            self.heartbeats.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn deregister(&self, _manifest: &AgentManifest) -> RegistryResult<()> {
            self.deregistrations.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn manifest() -> AgentManifest {
        let capability = Capability::builder(CapabilityId::new("mock.cap").unwrap())
            .name("Mock")
            .unwrap()
            .version("1.0.0")
            .unwrap()
            .add_scope("read:mock")
            .unwrap()
            .build()
            .unwrap();

        AgentManifest::builder(AgentId::random())
            .name("mock-agent")
            .unwrap()
            .version("0.1.0")
            .unwrap()
            .capabilities(vec![capability])
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn lifecycle_starts_and_stops_heartbeat() {
        let registry = Arc::new(MockRegistry {
            registers: Arc::new(AtomicUsize::new(0)),
            heartbeats: Arc::new(AtomicUsize::new(0)),
            deregistrations: Arc::new(AtomicUsize::new(0)),
        });

        let manifest = manifest();
        let config = RegistrationConfig::new(
            Duration::from_millis(10),
            Duration::from_millis(5),
            Duration::from_millis(20),
            NonZeroUsize::new(3).unwrap(),
        );

        let mut controller = RegistrationController::new(registry.clone(), manifest, config);
        let scheduler = TaskScheduler::default();

        controller
            .on_state_change(AgentState::Ready, &scheduler)
            .unwrap();

        tokio::time::sleep(Duration::from_millis(40)).await;

        assert!(registry.registers.load(Ordering::SeqCst) >= 1);
        assert!(registry.heartbeats.load(Ordering::SeqCst) >= 1);

        controller
            .on_state_change(AgentState::Retiring, &scheduler)
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(registry.deregistrations.load(Ordering::SeqCst) >= 1);
    }
}
