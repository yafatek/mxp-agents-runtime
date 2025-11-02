//! Procedural macros for agent tool definitions.
//!
//! The current implementation provides an identity `#[tool]` attribute so that
//! developers can begin annotating tool functions. A future phase will extend
//! this macro to emit schema metadata automatically.

use proc_macro::TokenStream;

/// Marks an async function as an MXP tool implementation.
///
/// The attribute does not alter the function body today; it exists to reserve
/// the public API surface and ease migration once code generation arrives.
#[proc_macro_attribute]
pub fn tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
