//! Minimal MXP agent example demonstrating the AgentKernel runtime.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use agent_kernel::{
    AgentKernel, AgentMessageHandler, AgentRegistry, HandlerContext, HandlerResult, LifecycleEvent,
    RegistrationConfig, SchedulerConfig, TaskScheduler,
};
use agent_primitives::{AgentId, AgentManifest, Capability, CapabilityId};
use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use tokio::signal::ctrl_c;
use tracing::info;

/// Example command-line arguments.
#[derive(Parser, Debug)]
struct Args {
    /// Optional agent name override.
    #[arg(long)]
    name: Option<String>,

    /// Heartbeat interval in seconds.
    #[arg(long, default_value_t = 10)]
    heartbeat: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let args = Args::parse();

    let agent_id = AgentId::random();
    let handler = Arc::new(EchoHandler);
    let scheduler = TaskScheduler::new(SchedulerConfig::default());

    let mut kernel = AgentKernel::new(agent_id, handler, scheduler);

    let manifest = AgentManifest::builder(agent_id)
        .name(args.name.unwrap_or_else(|| "example-agent".into()))?
        .version("0.1.0")?
        .description("Demonstration agent that echoes Call payloads")
        .capabilities(vec![echo_capability()?])
        .build()?;

    kernel.set_registry(
        Arc::new(LoggingRegistry),
        manifest,
        RegistrationConfig::new(
            Duration::from_secs(args.heartbeat),
            Duration::from_secs(1),
            Duration::from_secs(30),
            NonZeroUsize::new(3).expect("non-zero"),
        ),
    );

    kernel.transition(LifecycleEvent::Boot)?;
    kernel.transition(LifecycleEvent::Activate)?;

    info!("agent running; press Ctrl+C to terminate");
    ctrl_c().await?;

    kernel.transition(LifecycleEvent::Retire)?;
    kernel.transition(LifecycleEvent::Terminate)?;

    Ok(())
}

fn echo_capability() -> agent_primitives::Result<Capability> {
    Capability::builder(CapabilityId::new("echo.call")?)
        .name("Echo")?
        .version("1.0.0")?
        .add_scope("call:echo")?
        .build()
}

struct EchoHandler;

#[async_trait]
impl AgentMessageHandler for EchoHandler {
    async fn handle_call(&self, ctx: HandlerContext) -> HandlerResult {
        let payload = ctx.message().payload();
        info!(payload = ?payload, "received call payload");
        Ok(())
    }
}

struct LoggingRegistry;

#[async_trait]
impl AgentRegistry for LoggingRegistry {
    async fn register(&self, manifest: &AgentManifest) -> agent_kernel::RegistryResult<()> {
        info!(agent_id = %manifest.id(), "registered agent");
        Ok(())
    }

    async fn heartbeat(&self, manifest: &AgentManifest) -> agent_kernel::RegistryResult<()> {
        info!(agent_id = %manifest.id(), "heartbeat");
        Ok(())
    }

    async fn deregister(&self, manifest: &AgentManifest) -> agent_kernel::RegistryResult<()> {
        info!(agent_id = %manifest.id(), "deregistered agent");
        Ok(())
    }
}
