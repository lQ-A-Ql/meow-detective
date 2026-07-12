//! Windows Explorer thumbnail cache parser.

mod entries;
mod extractor;
mod header;

pub use extractor::ThumbcacheExtractor;

#[cfg(test)]
#[path = "../../tests/unit/thumbcache.rs"]
mod tests;
