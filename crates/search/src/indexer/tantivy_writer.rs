use crate::extractor::ExtractedText;
use std::path::Path;
use std::time::Instant;
use tantivy::{
    collector::{
        sort_key::{SortBySimilarityScore, SortByString},
        Count, TopDocs,
    },
    doc,
    query::QueryParser,
    schema::{Schema, Value, FAST, STORED, STRING, TEXT},
    DocAddress, Index, IndexWriter, Order, ReloadPolicy, Score, TantivyDocument, Term,
};
use thiserror::Error;

use super::index_identity::SearchIndexIdentity;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("Tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Query error: {0}")]
    Query(String),
    #[error("Index not open")]
    NotOpen,
    #[error("Schema error: {0}")]
    Schema(String),
    #[error("Search index identity error: {0}")]
    Identity(String),
}

pub type Result<T> = std::result::Result<T, IndexError>;

/// Number of documents processed per commit when indexing in batches.
/// Each commit makes the processed documents immediately searchable.
pub const CHUNK_COMMIT_INTERVAL: usize = 1000;

/// Statistics returned by chunked indexing operations.
#[derive(Debug, Clone)]
pub struct ChunkedIndexStats {
    /// Total number of documents successfully indexed.
    pub total_docs: u64,
    /// Number of commit operations performed.
    pub chunks_committed: u64,
    /// Wall-clock duration of the indexing operation.
    pub elapsed: std::time::Duration,
}

pub struct SearchIndex {
    pub(super) index: Index,
    pub(super) schema: Schema,
    pub(super) identity: SearchIndexIdentity,
}

pub(super) type StableTopDocs = Vec<((Score, Option<String>), DocAddress)>;

impl SearchIndex {
    pub fn create(path: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("file_id", STRING | STORED | FAST);
        schema_builder.add_text_field("path", TEXT | STORED);
        schema_builder.add_text_field("content", TEXT | STORED);
        schema_builder.add_text_field("name", TEXT | STORED);
        let schema = schema_builder.build();

        std::fs::create_dir_all(path)?;
        let directory = tantivy::directory::MmapDirectory::open(path)
            .map_err(|e| IndexError::Io(std::io::Error::other(e.to_string())))?;
        let index_exists = Index::exists(&directory)
            .map_err(|error| IndexError::Io(std::io::Error::other(error.to_string())))?;
        let (index, identity) = if index_exists {
            let index = Index::open(directory)?;
            let identity = SearchIndexIdentity::load(path)?;
            (index, identity)
        } else {
            let index = Index::open_or_create(directory, schema)?;
            let identity = SearchIndexIdentity::create(path)?;
            (index, identity)
        };
        let schema = index.schema();

        Ok(Self {
            index,
            schema,
            identity,
        })
    }

    pub fn open(path: &Path) -> Result<Self> {
        let index = Index::open_in_dir(path)?;
        let schema = index.schema();
        let identity = SearchIndexIdentity::load(path)?;
        Ok(Self {
            index,
            schema,
            identity,
        })
    }

    pub fn index_documents(
        &self,
        texts: &[ExtractedText],
        paths: &[(String, String)],
    ) -> Result<u64> {
        let mut writer: IndexWriter = self.index.writer(15_000_000)?;

        // Schema fields are defined in the constructor — these should never fail
        let file_id_field = self
            .schema
            .get_field("file_id")
            .map_err(|_| IndexError::Schema("missing file_id field".into()))?;
        let path_field = self
            .schema
            .get_field("path")
            .map_err(|_| IndexError::Schema("missing path field".into()))?;
        let content_field = self
            .schema
            .get_field("content")
            .map_err(|_| IndexError::Schema("missing content field".into()))?;
        let name_field = self
            .schema
            .get_field("name")
            .map_err(|_| IndexError::Schema("missing name field".into()))?;

        let path_map: std::collections::HashMap<&str, (&str, &str)> = paths
            .iter()
            .map(|(id, p)| (id.as_str(), (id.as_str(), p.as_str())))
            .collect();

        let mut count = 0u64;
        for text in texts {
            let (file_id, path) = path_map
                .get(text.file_id.as_str())
                .copied()
                .unwrap_or((&text.file_id, ""));
            writer.delete_term(Term::from_field_text(file_id_field, file_id));

            if !text.extractable || text.content.is_empty() {
                continue;
            }

            let name = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            writer.add_document(doc!(
                file_id_field => file_id,
                path_field => path,
                content_field => text.content.as_str(),
                name_field => name,
            ))?;
            count += 1;
        }

        writer.commit()?;
        Ok(count)
    }

    pub fn search_page(
        &self,
        query_str: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResult> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        let searcher = reader.searcher();

        let content_field = self
            .schema
            .get_field("content")
            .map_err(|_| IndexError::Schema("missing content field".into()))?;
        let file_id_field = self
            .schema
            .get_field("file_id")
            .map_err(|_| IndexError::Schema("missing file_id field".into()))?;
        let path_field = self
            .schema
            .get_field("path")
            .map_err(|_| IndexError::Schema("missing path field".into()))?;

        let query_parser = QueryParser::for_index(&self.index, vec![content_field]);

        let query = query_parser
            .parse_query(query_str)
            .or_else(|_| {
                let escaped = query_str.replace('"', "\\\"");
                let phrase = format!("\"{}\"", escaped);
                query_parser.parse_query(&phrase)
            })
            .map_err(|e| IndexError::Query(e.to_string()))?;

        if limit == 0 {
            let total_count = searcher.search(&query, &Count)?;
            return Ok(SearchResult {
                hits: Vec::new(),
                total_count: total_count as u64,
            });
        }

        if !self.supports_stable_paging() {
            return Err(IndexError::Schema(
                "search index does not contain the stable file_id sort field".to_string(),
            ));
        }
        let collector = TopDocs::with_limit(limit).and_offset(offset).order_by((
            (SortBySimilarityScore, Order::Desc),
            (SortByString::for_field("file_id"), Order::Asc),
        ));
        let (docs, total_count): (StableTopDocs, usize) =
            searcher.search(&query, &(collector, Count))?;
        let top_docs = docs
            .into_iter()
            .map(|((score, _), address)| (score, address));

        let mut hits = Vec::new();
        for (score, doc_addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_addr)?;
            let file_id = doc
                .get_first(file_id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = doc
                .get_first(path_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let content = doc
                .get_first(content_field)
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let snippets = crate::highlighter::highlight(content, query_str);

            hits.push(SearchHit {
                file_id,
                path,
                score: score as f64,
                snippets,
            });
        }

        Ok(SearchResult {
            hits,
            total_count: total_count as u64,
        })
    }

    pub fn supports_stable_paging(&self) -> bool {
        self.validate_search_schema().is_ok()
    }

    pub fn validate_search_schema(&self) -> Result<()> {
        self.validate_text_field("file_id", true, true, true)?;
        self.validate_text_field("path", false, true, false)?;
        self.validate_text_field("content", true, true, false)?;
        Ok(())
    }

    pub fn snapshot_opstamp(&self) -> Result<u64> {
        Ok(self.index.load_metas()?.opstamp)
    }

    pub fn generation(&self) -> &str {
        self.identity.generation()
    }

    pub fn schema_version(&self) -> u32 {
        self.identity.schema_version()
    }

    fn validate_text_field(
        &self,
        name: &str,
        indexed: bool,
        stored: bool,
        fast: bool,
    ) -> Result<()> {
        let field = self
            .schema
            .get_field(name)
            .map_err(|_| IndexError::Schema(format!("missing {name} field")))?;
        let entry = self.schema.get_field_entry(field);
        if !entry.field_type().is_str() {
            return Err(IndexError::Schema(format!("{name} field must be a string")));
        }
        if indexed && !entry.is_indexed() {
            return Err(IndexError::Schema(format!("{name} field must be indexed")));
        }
        if stored && !entry.is_stored() {
            return Err(IndexError::Schema(format!("{name} field must be stored")));
        }
        if fast && !entry.is_fast() {
            return Err(IndexError::Schema(format!(
                "{name} field must be a fast field"
            )));
        }
        Ok(())
    }

    pub fn search(&self, query_str: &str, limit: usize) -> Result<SearchResult> {
        self.search_page(query_str, 0, limit)
    }

    /// Index files in batches of [`CHUNK_COMMIT_INTERVAL`], committing after
    /// each batch so that partial results are searchable without waiting for
    /// the full run to finish.
    ///
    /// `progress_cb` is called after each batch commit with `(completed, total)`.
    pub fn index_files_chunked(
        &self,
        texts: &[ExtractedText],
        paths: &[(String, String)],
        progress_cb: Option<&dyn Fn(u64, u64)>,
    ) -> Result<ChunkedIndexStats> {
        let start = Instant::now();
        let total = texts.len() as u64;
        let mut total_docs = 0u64;
        let mut chunks_committed = 0u64;

        for batch_start in (0..texts.len()).step_by(CHUNK_COMMIT_INTERVAL) {
            let batch_end = std::cmp::min(batch_start + CHUNK_COMMIT_INTERVAL, texts.len());
            let batch_texts = &texts[batch_start..batch_end];
            let batch_paths = &paths[batch_start..batch_end];

            let count = self.index_documents(batch_texts, batch_paths)?;
            total_docs += count;
            chunks_committed += 1;

            if let Some(cb) = progress_cb {
                cb(total_docs, total);
            }
        }

        Ok(ChunkedIndexStats {
            total_docs,
            chunks_committed,
            elapsed: start.elapsed(),
        })
    }

    /// Incrementally index only files whose `file_id` is not already present in
    /// the index. Files that already exist are silently skipped.
    ///
    /// `existing_count` is a hint — the current index size — used for caller
    /// awareness; the method queries the index directly to decide which files
    /// to skip.
    pub fn index_files_incremental(
        &self,
        new_files: &[ExtractedText],
        new_paths: &[(String, String)],
        _existing_count: u64,
    ) -> Result<ChunkedIndexStats> {
        let start = Instant::now();

        // Build a reader that sees the latest committed data.
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        let searcher = reader.searcher();

        let file_id_field = self
            .schema
            .get_field("file_id")
            .map_err(|_| IndexError::Schema("missing file_id field".into()))?;

        // Filter out files whose file_id already appears in the index.
        let mut filtered_texts = Vec::new();
        let mut filtered_paths = Vec::new();

        for (text, path_pair) in new_files.iter().zip(new_paths.iter()) {
            let term = Term::from_field_text(file_id_field, &text.file_id);
            let freq = searcher.doc_freq(&term).unwrap_or(0);
            if freq > 0 {
                continue; // already indexed — skip
            }
            filtered_texts.push(text.clone());
            filtered_paths.push(path_pair.clone());
        }

        let total_processed = self.index_documents(&filtered_texts, &filtered_paths)?;

        Ok(ChunkedIndexStats {
            total_docs: total_processed,
            chunks_committed: if total_processed > 0 { 1 } else { 0 },
            elapsed: start.elapsed(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub total_count: u64,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub file_id: String,
    pub path: String,
    pub score: f64,
    pub snippets: Vec<SearchSnippet>,
}

#[derive(Debug, Clone)]
pub struct SearchSnippet {
    pub text: String,
    pub highlights: Vec<SearchHighlight>,
}

#[derive(Debug, Clone)]
pub struct SearchHighlight {
    pub start: u32,
    pub end: u32,
}

#[cfg(test)]
#[path = "../../tests/unit/indexer/tantivy_writer.rs"]
mod tests;
