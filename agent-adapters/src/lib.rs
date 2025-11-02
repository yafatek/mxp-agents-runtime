//! Model and service adapters used by agents.
//!
//! Each module exposes implementations for a specific provider while sharing a
//! common trait-based interface defined in [`traits`].

#![warn(missing_docs, clippy::pedantic)]

pub mod anthropic;
pub mod gemini;
pub mod mxp_model;
pub mod ollama;
pub mod openai;
pub mod traits;

mod http_client;
