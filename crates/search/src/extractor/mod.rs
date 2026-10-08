pub mod text_extractor;

pub use text_extractor::{
    classify_text_bytes, extract_text, ExtractedText, TextContentStatus, CONTENT_SNIFF_BYTES,
};
