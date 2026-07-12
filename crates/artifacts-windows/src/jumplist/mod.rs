//! Windows AutomaticDestinations and CustomDestinations parser.

mod embedded_lnk;
mod extractor;

pub use extractor::JumpListExtractor;

#[cfg(test)]
#[path = "../../tests/unit/jumplist.rs"]
mod tests;
