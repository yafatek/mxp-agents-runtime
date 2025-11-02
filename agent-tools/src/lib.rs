//! Tool discovery and capability enforcement utilities.
//!
//! The modules exposed here make it possible to register annotated tool
//! functions, associate capability metadata, and invoke them at runtime. Future
//! phases will extend the sandbox implementation to carry out isolation.

#![warn(missing_docs, clippy::pedantic)]

pub mod macros;
pub mod registry;
pub mod sandbox;
