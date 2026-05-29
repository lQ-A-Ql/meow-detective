pub mod extractor;
pub mod highlighter;
pub mod indexer;

pub use extractor::{extract_text, ExtractedText};
pub use highlighter::highlight;
pub use indexer::{SearchHighlight, SearchHit, SearchIndex, SearchResult, SearchSnippet};
