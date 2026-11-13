//! Windows AutomaticDestinations and CustomDestinations parser.

mod extractor;

pub use extractor::JumpListExtractor;

#[cfg(test)]
#[path = "../../tests/unit/jumplist.rs"]
mod tests;
