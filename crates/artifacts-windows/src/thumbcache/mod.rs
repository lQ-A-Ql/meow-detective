//! Windows Explorer thumbnail cache parser.

mod extractor;

pub use extractor::ThumbcacheExtractor;

#[cfg(test)]
#[path = "../../tests/unit/thumbcache.rs"]
mod tests;
