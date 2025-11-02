//! Minimal MXP agent example demonstrating the AgentKernel runtime.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use agent_adapters::ollama::{OllamaAdapter, OllamaConfig};
use agent_kernel::{
    AgentKernel, AgentRegistry, CallOutcomeSink, KernelMessageHandler, LifecycleEvent,
    RegistrationConfig, SchedulerConfig, TaskScheduler, TracingCallSink,
};
use agent_memory::{FileJournal, MemoryBusBuilder, VolatileConfig};
use agent_primitives::{AgentId, AgentManifest, Capability, CapabilityId};
use agent_tools::macros::tool;
use agent_tools::registry::{ToolMetadata, ToolRegistry, ToolResult};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use serde_json::Value;
use tokio::signal::ctrl_c;
use tracing::{info, warn};

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
    let handler = build_handler(agent_id).await?;
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

#[tool]
async fn echo_tool(input: Value) -> ToolResult<Value> {
    Ok(input)
}

async fn build_handler(agent_id: AgentId) -> Result<Arc<KernelMessageHandler>> {
    let adapter = Arc::new(
        OllamaAdapter::new(OllamaConfig::new("gemma3")).map_err(|err| anyhow!(err.to_string()))?,
    );

    let tools = Arc::new(ToolRegistry::new());
    let tool_capability = CapabilityId::new("tool.echo")?;
    let metadata = ToolMetadata::new("echo", "1.0.0")
        .map_err(|err| anyhow!(err.to_string()))?
        .with_description("Echo tool for demonstration")
        .with_capabilities(vec![tool_capability]);

    tools
        .register_tool(metadata, echo_tool)
        .map_err(|err| anyhow!(err.to_string()))?;

    let sink: Arc<dyn CallOutcomeSink> = Arc::new(TracingCallSink);

    let journal_path = std::env::temp_dir().join(format!("mxp-agent-{}-journal.log", agent_id));
    let journal: Arc<dyn agent_memory::Journal> = Arc::new(
        FileJournal::open(&journal_path)
            .await
            .map_err(|err| anyhow!(err.to_string()))?,
    );
    let memory_bus = Arc::new(
        MemoryBusBuilder::new(VolatileConfig::default())
            .with_journal(journal)
            .build()
            .map_err(|err| anyhow!(err.to_string()))?,
    );

    info!(journal = %journal_path.display(), "memory journal initialised");

    Ok(Arc::new(
        KernelMessageHandler::new(adapter, tools, sink).with_memory(memory_bus),
    ))
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
        warn!(agent_id = %manifest.id(), "deregistered agent");
        Ok(())
    }
}
