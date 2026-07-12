//! Mbox format support.
//!
//! Public entry points stay here as a thin facade. The implementation lives
//! in the `framing`, `header`, and `mime` submodules.

mod framing;
mod header;
mod mime;

pub use framing::{detect_variant, parse_mbox, MboxVariant};

#[cfg(test)]
#[path = "../tests/unit/mbox/mod.rs"]
mod tests;
