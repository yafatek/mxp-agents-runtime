//! Minimal MXP agent example demonstrating the AgentKernel runtime.

use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use agent_adapters::ollama::{OllamaAdapter, OllamaConfig};
use agent_kernel::{
    AgentKernel, AgentRegistry, AuditEmitter, CallOutcomeSink, CompositeAuditEmitter,
    CompositePolicyObserver, GovernanceAuditEmitter, KernelMessageHandler, LifecycleEvent,
    MxpAuditObserver, PolicyObserver, RegistrationConfig, SchedulerConfig, TaskScheduler,
    TracingAuditEmitter, TracingCallSink, TracingPolicyObserver,
};
use agent_memory::{FileJournal, MemoryBusBuilder, VolatileConfig};
use agent_policy::{PolicyDecision, PolicyRule, RuleBasedEngine, RuleMatcher};
use agent_primitives::{AgentId, AgentManifest, Capability, CapabilityId};
use agent_tools::macros::tool;
use agent_tools::registry::{ToolRegistry, ToolResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use clap::Parser;
use mxp::Transport;
use serde::{Deserialize, Serialize};
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

    /// Optional governance agent socket address (e.g. 127.0.0.1:9100).
    #[arg(long)]
    governance: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let args = Args::parse();

    let agent_id = AgentId::random();
    let governance_addr = if let Some(addr) = args.governance.as_ref() {
        Some(
            addr.parse::<SocketAddr>()
                .map_err(|err| anyhow!(err.to_string()))?,
        )
    } else {
        None
    };

    let handler = build_handler(agent_id, governance_addr).await?;
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

#[derive(Debug, Deserialize)]
struct EchoRequest {
    message: String,
}

#[derive(Debug, Serialize)]
struct EchoResponse {
    message: String,
}

#[tool(
    name = "echo",
    version = "1.0.0",
    description = "Echo tool for demonstration",
    capabilities = ["tool.echo"],
)]
async fn echo_tool(input: EchoRequest) -> ToolResult<EchoResponse> {
    Ok(EchoResponse {
        message: input.message,
    })
}

async fn build_handler(
    agent_id: AgentId,
    governance_addr: Option<SocketAddr>,
) -> Result<Arc<KernelMessageHandler>> {
    let adapter = Arc::new(
        OllamaAdapter::new(OllamaConfig::new("gemma3")).map_err(|err| anyhow!(err.to_string()))?,
    );

    let tools = Arc::new(ToolRegistry::new());
    register_echo_tool(tools.as_ref()).map_err(|err| anyhow!(err.to_string()))?;

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

    let (policy_engine, policy_observer) = {
        let engine = RuleBasedEngine::new(PolicyDecision::allow());
        let rule = PolicyRule::new(
            "deny-dangerous",
            RuleMatcher::for_tool("rm_all"),
            PolicyDecision::deny("dangerous tool disabled in development"),
        )
        .map_err(|err| anyhow!(err.to_string()))?;
        engine.add_rule(rule);
        let mut audit_emitters: Vec<Arc<dyn AuditEmitter>> =
            vec![Arc::new(TracingAuditEmitter) as Arc<dyn AuditEmitter>];
        if let Some(addr) = governance_addr {
            let transport = Transport::default();
            let bind_addr: SocketAddr =
                "0.0.0.0:0".parse().expect("valid ephemeral socket address");
            let handle = transport
                .bind(bind_addr)
                .map_err(|err| anyhow!(format!("failed to bind audit transport: {err:?}")))?;
            audit_emitters.push(Arc::new(GovernanceAuditEmitter::new(handle, addr)) as Arc<_>);
            info!(%addr, "governance audit emitter enabled");
        }
        let audit_emitter: Arc<dyn AuditEmitter> = if audit_emitters.len() == 1 {
            Arc::clone(&audit_emitters[0])
        } else {
            Arc::new(CompositeAuditEmitter::new(audit_emitters))
        };

        let observer = CompositePolicyObserver::new([
            Arc::new(TracingPolicyObserver) as Arc<dyn PolicyObserver>,
            Arc::new(MxpAuditObserver::new(audit_emitter)) as Arc<dyn PolicyObserver>,
        ]);
        (
            Arc::new(engine),
            Arc::new(observer) as Arc<dyn PolicyObserver>,
        )
    };

    Ok(Arc::new(
        KernelMessageHandler::new(adapter, tools, sink)
            .with_memory(memory_bus)
            .with_policy(policy_engine)
            .with_policy_observer(policy_observer),
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
