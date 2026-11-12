//! Facade over `import-core`; keeps `crate::import_pipeline::*` paths stable.
pub use ::import_core::import_pipeline::*;

#[cfg(test)]
#[path = "../tests/unit/import_pipeline/mod.rs"]
mod tests;
