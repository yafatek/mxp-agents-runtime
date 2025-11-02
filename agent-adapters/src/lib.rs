//! Model and service adapters used by agents.
//!
//! Phase 0 scaffolding: concrete clients are introduced in later phases.

#![warn(missing_docs, clippy::pedantic)]

pub mod traits {
    //! Common traits shared by all adapters.
}

pub mod openai {
    //! `OpenAI` adapter implementation.
}

pub mod anthropic {
    //! `Anthropic` adapter implementation.
}

pub mod gemini {
    //! `Gemini` adapter implementation.
}

pub mod ollama {
    //! `Ollama` adapter implementation.
}

pub mod mxp_model {
    //! Native MXP-hosted model integration.
}
