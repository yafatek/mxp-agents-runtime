//! Routing utilities for MXP protocol messages.

use std::sync::Arc;
use std::time::Instant;

use agent_primitives::AgentId;
use async_trait::async_trait;
use mxp::{Message, MessageType};
use thiserror::Error;

/// Context provided to message handlers.
#[derive(Debug, Clone)]
pub struct HandlerContext {
    agent_id: AgentId,
    received_at: Instant,
    message: Arc<Message>,
}

impl HandlerContext {
    /// Constructs a context from an owned message.
    #[must_use]
    pub fn from_message(agent_id: AgentId, message: Message) -> Self {
        Self::from_shared(agent_id, Arc::new(message))
    }

    /// Constructs a context from a shared message instance.
    #[must_use]
    pub fn from_shared(agent_id: AgentId, message: Arc<Message>) -> Self {
        Self {
            agent_id,
            received_at: Instant::now(),
            message,
        }
    }

    /// Returns the agent identifier.
    #[must_use]
    pub const fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Returns the time the message was received.
    #[must_use]
    pub fn received_at(&self) -> Instant {
        self.received_at
    }

    /// Returns the underlying MXP message.
    #[must_use]
    pub fn message(&self) -> &Message {
        &self.message
    }

    /// Returns the MXP message type.
    ///
    /// # Errors
    ///
    /// Returns [`HandlerError::MissingMessageType`] when the header could not be
    /// decoded into a [`MessageType`].
    pub fn message_type(&self) -> HandlerResult<MessageType> {
        self.message
            .message_type()
            .ok_or(HandlerError::MissingMessageType)
    }
}

/// Errors that can occur during message handling.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HandlerError {
    /// The message header did not contain a valid message type.
    #[error("message missing type information")]
    MissingMessageType,
    /// The agent does not handle the message type.
    #[error("message type {0:?} is not supported")]
    Unsupported(MessageType),
    /// Custom handler error with human-readable context.
    #[error("handler error: {0}")]
    Custom(String),
}

impl HandlerError {
    /// Creates a custom error variant from a string-like value.
    #[must_use]
    pub fn custom(reason: impl Into<String>) -> Self {
        Self::Custom(reason.into())
    }
}

/// Result alias for handler operations.
pub type HandlerResult<T = ()> = Result<T, HandlerError>;

/// Trait implemented by agent-specific MXP message handlers.
#[async_trait]
pub trait AgentMessageHandler: Send + Sync {
    /// Called for `AgentRegister` messages.
    async fn handle_agent_register(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::AgentRegister).await
    }

    /// Called for `AgentDiscover` messages.
    async fn handle_agent_discover(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::AgentDiscover).await
    }

    /// Called for `AgentHeartbeat` messages.
    async fn handle_agent_heartbeat(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::AgentHeartbeat)
            .await
    }

    /// Called for `Call` messages.
    async fn handle_call(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::Call).await
    }

    /// Called for `Response` messages.
    async fn handle_response(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::Response).await
    }

    /// Called for `Event` messages.
    async fn handle_event(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::Event).await
    }

    /// Called for `StreamOpen` messages.
    async fn handle_stream_open(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::StreamOpen).await
    }

    /// Called for `StreamChunk` messages.
    async fn handle_stream_chunk(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::StreamChunk).await
    }

    /// Called for `StreamClose` messages.
    async fn handle_stream_close(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::StreamClose).await
    }

    /// Called for `Ack` messages.
    async fn handle_ack(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::Ack).await
    }

    /// Called for protocol-level error responses.
    async fn handle_error(&self, ctx: HandlerContext) -> HandlerResult {
        self.handle_unhandled(ctx, MessageType::Error).await
    }

    /// Fallback invoked when a specialized handler is not implemented.
    async fn handle_unhandled(
        &self,
        ctx: HandlerContext,
        message_type: MessageType,
    ) -> HandlerResult {
        let _ = ctx;
        Err(HandlerError::Unsupported(message_type))
    }
}

/// Dispatches a message to the appropriate handler.
///
/// # Errors
///
/// Propagates errors returned by the underlying handler implementation.
pub async fn dispatch_message<H>(handler: &H, ctx: HandlerContext) -> HandlerResult
where
    H: AgentMessageHandler + ?Sized,
{
    let message_type = ctx.message_type()?;

    match message_type {
        MessageType::AgentRegister => handler.handle_agent_register(ctx).await,
        MessageType::AgentDiscover => handler.handle_agent_discover(ctx).await,
        MessageType::AgentHeartbeat => handler.handle_agent_heartbeat(ctx).await,
        MessageType::Call => handler.handle_call(ctx).await,
        MessageType::Response => handler.handle_response(ctx).await,
        MessageType::Event => handler.handle_event(ctx).await,
        MessageType::StreamOpen => handler.handle_stream_open(ctx).await,
        MessageType::StreamChunk => handler.handle_stream_chunk(ctx).await,
        MessageType::StreamClose => handler.handle_stream_close(ctx).await,
        MessageType::Ack => handler.handle_ack(ctx).await,
        MessageType::Error => handler.handle_error(ctx).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingHandler {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AgentMessageHandler for CountingHandler {
        async fn handle_call(&self, _ctx: HandlerContext) -> HandlerResult {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn dispatches_to_specific_handler() {
        let handler = CountingHandler {
            calls: Arc::new(AtomicUsize::new(0)),
        };

        let message = Message::new(MessageType::Call, b"ping");
        let ctx = HandlerContext::from_message(AgentId::random(), message);
        dispatch_message(&handler, ctx).await.unwrap();

        assert_eq!(handler.calls.load(Ordering::SeqCst), 1);
    }

    struct UnsupportedHandler;

    #[async_trait]
    impl AgentMessageHandler for UnsupportedHandler {}

    #[tokio::test]
    async fn unsupported_message_errors() {
        let handler = UnsupportedHandler;
        let message = Message::new(MessageType::Event, b"noop");
        let ctx = HandlerContext::from_message(AgentId::random(), message);
        let err = dispatch_message(&handler, ctx)
            .await
            .expect_err("should error");

        assert_eq!(err, HandlerError::Unsupported(MessageType::Event));
    }
}
