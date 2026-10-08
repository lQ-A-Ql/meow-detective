//! Bounded decoding of WOF file-provider streams (Compact OS).
mod bits;
mod codec;
mod error;
mod format;
mod huffman;
mod lzx;
mod lzx_trees;
mod stream;
mod xpress;

use error::{Result, WofError};
pub(crate) use format::WofAlgorithm;
pub(crate) use stream::WofStream;

#[cfg(test)]
#[path = "../../tests/unit/wof/mod.rs"]
mod tests;
