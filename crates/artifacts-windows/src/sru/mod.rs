//! Windows SRU ESE database metadata.

mod extractor;

pub use extractor::SruExtractor;

#[cfg(test)]
#[path = "../../tests/unit/sru.rs"]
mod tests;
