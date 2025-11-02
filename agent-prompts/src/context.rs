//! Context window management with intelligent compression strategies.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Result alias for context operations.
pub type ContextResult<T> = Result<T, ContextError>;

/// Errors that can occur during context window management.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    /// Context window budget exceeded.
    #[error("context window budget exceeded: {current} > {max} tokens")]
    BudgetExceeded {
        /// Current token count.
        current: usize,
        /// Maximum allowed tokens.
        max: usize,
    },

    /// Compression failed.
    #[error("compression failed: {reason}")]
    CompressionError {
        /// Reason for the failure.
        reason: String,
    },
}

/// A single message in the context window.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextMessage {
    /// Role of the message author.
    pub role: String,
    /// Message content.
    pub content: String,
    /// Estimated token count (approximate).
    pub estimated_tokens: usize,
    /// Importance score (0-100, higher = more important).
    pub importance: u8,
    /// Whether this message should be pinned (never compressed).
    pub pinned: bool,
}

impl ContextMessage {
    /// Creates a new context message.
    #[must_use]
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let estimated_tokens = estimate_tokens(&content);
        Self {
            role: role.into(),
            content,
            estimated_tokens,
            importance: 50, // Default medium importance
            pinned: false,
        }
    }

    /// Sets the importance score (0-100).
    #[must_use]
    pub fn with_importance(mut self, importance: u8) -> Self {
        self.importance = importance.min(100);
        self
    }

    /// Marks this message as pinned (never compressed).
    #[must_use]
    pub fn pinned(mut self) -> Self {
        self.pinned = true;
        self
    }
}

/// Configuration for context window management.
#[derive(Clone, Debug)]
pub struct ContextWindowConfig {
    /// Maximum tokens allowed in the context window.
    pub max_tokens: usize,
    /// Number of recent messages to always keep.
    pub recent_window_size: usize,
    /// Minimum importance score to preserve during compression (0-100).
    pub min_importance_threshold: u8,
    /// Whether to enable automatic summarization.
    pub enable_summarization: bool,
}

impl Default for ContextWindowConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8192,
            recent_window_size: 10,
            min_importance_threshold: 30,
            enable_summarization: true,
        }
    }
}

/// Manages context window with intelligent compression.
///
/// Uses a three-tier strategy:
/// 1. **Recent window**: Last N messages always included
/// 2. **Compressed history**: Older messages summarized or filtered
/// 3. **Pinned messages**: High-importance messages never removed
///
/// # Examples
///
/// ```
/// use agent_prompts::context::{ContextWindowManager, ContextWindowConfig, ContextMessage};
///
/// let config = ContextWindowConfig {
///     max_tokens: 1000,
///     recent_window_size: 5,
///     ..Default::default()
/// };
///
/// let mut manager = ContextWindowManager::new(config);
/// manager.add_message(ContextMessage::new("user", "Hello"));
/// manager.add_message(ContextMessage::new("assistant", "Hi there!"));
///
/// let messages = manager.get_messages();
/// assert_eq!(messages.len(), 2);
/// ```
pub struct ContextWindowManager {
    config: ContextWindowConfig,
    messages: VecDeque<ContextMessage>,
    summarized_history: Option<String>,
    current_tokens: usize,
}

impl ContextWindowManager {
    /// Creates a new context window manager with the supplied configuration.
    #[must_use]
    pub fn new(config: ContextWindowConfig) -> Self {
        Self {
            config,
            messages: VecDeque::new(),
            summarized_history: None,
            current_tokens: 0,
        }
    }

    /// Adds a message to the context window.
    ///
    /// If adding the message would exceed the budget, compression is triggered.
    pub fn add_message(&mut self, message: ContextMessage) {
        self.current_tokens += message.estimated_tokens;
        self.messages.push_back(message);

        if self.current_tokens > self.config.max_tokens {
            self.compress();
        }
    }

    /// Returns all messages in the context window.
    #[must_use]
    pub fn get_messages(&self) -> Vec<ContextMessage> {
        self.messages.iter().cloned().collect()
    }

    /// Returns the summarized history if available.
    #[must_use]
    pub fn summarized_history(&self) -> Option<&str> {
        self.summarized_history.as_deref()
    }

    /// Returns the current token count.
    #[must_use]
    pub const fn current_tokens(&self) -> usize {
        self.current_tokens
    }

    /// Returns the maximum token budget.
    #[must_use]
    pub const fn max_tokens(&self) -> usize {
        self.config.max_tokens
    }

    /// Clears all messages from the context window.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.summarized_history = None;
        self.current_tokens = 0;
    }

    /// Compresses the context window to fit within the budget.
    ///
    /// Uses a multi-strategy approach:
    /// 1. Keep recent window intact
    /// 2. Remove low-importance messages
    /// 3. Summarize older messages if enabled
    fn compress(&mut self) {
        // Strategy 1: Remove low-importance messages from the middle
        let recent_count = self.config.recent_window_size.min(self.messages.len());
        let mut to_remove = Vec::new();

        for (idx, msg) in self.messages.iter().enumerate() {
            // Skip recent messages and pinned messages
            if idx >= self.messages.len() - recent_count || msg.pinned {
                continue;
            }

            // Mark low-importance messages for removal
            if msg.importance < self.config.min_importance_threshold {
                to_remove.push(idx);
            }
        }

        // Remove marked messages (in reverse to maintain indices)
        for idx in to_remove.iter().rev() {
            if let Some(removed) = self.messages.remove(*idx) {
                self.current_tokens = self.current_tokens.saturating_sub(removed.estimated_tokens);
            }
        }

        // Strategy 2: If still over budget, summarize older messages
        if self.current_tokens > self.config.max_tokens && self.config.enable_summarization {
            self.summarize_older_messages();
        }

        // Strategy 3: If still over budget, remove oldest messages
        while self.current_tokens > self.config.max_tokens && self.messages.len() > recent_count {
            if let Some(removed) = self.messages.pop_front() {
                if removed.pinned {
                    // Put pinned message back
                    self.messages.push_front(removed);
                    break;
                }
                self.current_tokens = self.current_tokens.saturating_sub(removed.estimated_tokens);
            }
        }
    }

    /// Summarizes older messages into a compact history.
    fn summarize_older_messages(&mut self) {
        let recent_count = self.config.recent_window_size.min(self.messages.len());
        if self.messages.len() <= recent_count {
            return;
        }

        // Extract messages to summarize (excluding recent window and pinned)
        let mut to_summarize = Vec::new();
        let mut new_messages = VecDeque::new();

        for (idx, msg) in self.messages.iter().enumerate() {
            if idx >= self.messages.len() - recent_count || msg.pinned {
                new_messages.push_back(msg.clone());
            } else {
                to_summarize.push(msg.clone());
            }
        }

        if !to_summarize.is_empty() {
            // Create a simple summary (in production, this could use an LLM)
            let summary = create_simple_summary(&to_summarize);
            self.summarized_history = Some(summary.clone());

            // Update token count
            for msg in &to_summarize {
                self.current_tokens = self.current_tokens.saturating_sub(msg.estimated_tokens);
            }
            self.current_tokens += estimate_tokens(&summary);

            self.messages = new_messages;
        }
    }
}

/// Estimates the token count for a given text.
///
/// Uses a simple heuristic: ~4 characters per token (average for English).
/// For production, consider using a proper tokenizer like `tiktoken`.
fn estimate_tokens(text: &str) -> usize {
    // Simple heuristic: 1 token ≈ 4 characters
    (text.len() / 4).max(1)
}

/// Creates a simple summary of messages.
///
/// In production, this should use an LLM for better summarization.
fn create_simple_summary(messages: &[ContextMessage]) -> String {
    use std::fmt::Write;

    let mut summary = String::from("[Earlier conversation summary]\n");

    // Group by role and count
    let mut user_count = 0;
    let mut assistant_count = 0;
    let mut tool_count = 0;

    for msg in messages {
        match msg.role.as_str() {
            "user" => user_count += 1,
            "assistant" => assistant_count += 1,
            "tool" => tool_count += 1,
            _ => {}
        }
    }

    let _ = write!(
        summary,
        "Exchanged {user_count} user messages, {assistant_count} assistant responses"
    );

    if tool_count > 0 {
        let _ = write!(summary, ", {tool_count} tool calls");
    }

    summary.push('.');
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_messages_within_budget() {
        let config = ContextWindowConfig {
            max_tokens: 1000,
            recent_window_size: 5,
            ..Default::default()
        };
        let mut manager = ContextWindowManager::new(config);

        manager.add_message(ContextMessage::new("user", "Hello"));
        manager.add_message(ContextMessage::new("assistant", "Hi there!"));

        assert_eq!(manager.get_messages().len(), 2);
    }

    #[test]
    fn compresses_when_over_budget() {
        let config = ContextWindowConfig {
            max_tokens: 50, // Small budget
            recent_window_size: 3,
            min_importance_threshold: 50,
            enable_summarization: false,
        };
        let mut manager = ContextWindowManager::new(config);

        // Add messages with enough content to exceed budget
        // Each message: "This is a low importance test message number X" = ~48 chars = 12 tokens
        for i in 0..10 {
            manager.add_message(
                ContextMessage::new(
                    "user",
                    format!("This is a low importance test message number {i}"),
                )
                .with_importance(30),
            );
        }

        // Should have compressed to stay within budget
        assert!(
            manager.current_tokens() <= manager.max_tokens(),
            "Token count {} exceeds max {}",
            manager.current_tokens(),
            manager.max_tokens()
        );
        // Should have removed some messages (10 messages * 12 tokens = 120 tokens > 50 budget)
        assert!(
            manager.get_messages().len() < 10,
            "Expected fewer than 10 messages, got {}",
            manager.get_messages().len()
        );
    }

    #[test]
    fn preserves_pinned_messages() {
        let config = ContextWindowConfig {
            max_tokens: 50,
            recent_window_size: 1,
            ..Default::default()
        };
        let mut manager = ContextWindowManager::new(config);

        let pinned = ContextMessage::new("system", "Important context").pinned();
        manager.add_message(pinned.clone());

        // Add many more messages
        for i in 0..20 {
            manager.add_message(ContextMessage::new("user", format!("Message {i}")));
        }

        // Pinned message should still be present
        let messages = manager.get_messages();
        assert!(messages.iter().any(|m| m.content == "Important context"));
    }

    #[test]
    fn estimates_tokens_reasonably() {
        let short = "Hello";
        let long = "This is a much longer message with many more words.";

        assert!(estimate_tokens(long) > estimate_tokens(short));
    }

    #[test]
    fn creates_summary() {
        let messages = vec![
            ContextMessage::new("user", "Question 1"),
            ContextMessage::new("assistant", "Answer 1"),
            ContextMessage::new("user", "Question 2"),
        ];

        let summary = create_simple_summary(&messages);
        assert!(summary.contains("2 user messages"));
        assert!(summary.contains("1 assistant response"));
    }

    #[test]
    fn clears_all_state() {
        let mut manager = ContextWindowManager::new(ContextWindowConfig::default());
        manager.add_message(ContextMessage::new("user", "Hello"));
        manager.clear();

        assert_eq!(manager.get_messages().len(), 0);
        assert_eq!(manager.current_tokens(), 0);
    }
}
