//! exFAT filesystem reader.
//!
//! Implements the `FileSystemReader` trait for exFAT formatted volumes.
//! Based on the Microsoft exFAT specification.

pub mod boot;
mod data;
pub mod dir;
pub mod fat;
mod filesystem;
mod navigation;
mod reader;
pub mod types;

pub use reader::ExfatReader;

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
