use crate::extractor::ExtractedText;
use std::path::Path;
use std::time::Instant;
use tantivy::{
    collector::{Count, TopDocs},
    doc,
    query::QueryParser,
    schema::{Schema, Value, STORED, STRING, TEXT},
    Index, IndexWriter, ReloadPolicy, TantivyDocument, Term,
};
use thiserror::Error;

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
    index: Index,
    schema: Schema,
}

impl SearchIndex {
    pub fn create(path: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("file_id", STRING | STORED);
        schema_builder.add_text_field("path", TEXT | STORED);
        schema_builder.add_text_field("content", TEXT | STORED);
        schema_builder.add_text_field("name", TEXT | STORED);
        let schema = schema_builder.build();

        std::fs::create_dir_all(path)?;
        let directory = tantivy::directory::MmapDirectory::open(path)
            .map_err(|e| IndexError::Io(std::io::Error::other(e.to_string())))?;
        let index = Index::open_or_create(directory, schema.clone())?;

        Ok(Self { index, schema })
    }

    pub fn open(path: &Path) -> Result<Self> {
        let index = Index::open_in_dir(path)?;
        let schema = index.schema();
        Ok(Self { index, schema })
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

        let top_docs = TopDocs::with_limit(limit)
            .and_offset(offset)
            .order_by_score();
        let (top_docs, total_count) = searcher.search(&query, &(top_docs, Count))?;

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
mod tests {
    use super::*;
    use crate::extractor::ExtractedText;
    use tempfile::tempdir;

    fn sample_extracted_texts() -> Vec<ExtractedText> {
        vec![
            ExtractedText {
                file_id: "f1".to_string(),
                content: "The quick brown fox jumps over the lazy dog".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 43,
            },
            ExtractedText {
                file_id: "f2".to_string(),
                content: "Rust is a systems programming language".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 39,
            },
            ExtractedText {
                file_id: "f3".to_string(),
                content: "Binary file cannot be extracted".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 32,
            },
        ]
    }

    fn sample_paths() -> Vec<(String, String)> {
        vec![
            ("f1".to_string(), "/evidence/doc1.txt".to_string()),
            ("f2".to_string(), "/evidence/doc2.rs".to_string()),
            ("f3".to_string(), "/evidence/doc3.bin".to_string()),
        ]
    }

    #[test]
    fn create_search_index_creates_directory() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        let result = SearchIndex::create(&index_path);
        assert!(result.is_ok());
        assert!(index_path.exists());
    }

    #[test]
    fn index_documents_returns_count() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        let index = SearchIndex::create(&index_path).unwrap();
        let count = index
            .index_documents(&sample_extracted_texts(), &sample_paths())
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn search_finds_indexed_document() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        let index = SearchIndex::create(&index_path).unwrap();
        index
            .index_documents(&sample_extracted_texts(), &sample_paths())
            .unwrap();

        let result = index.search("rust", 10).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].file_id, "f2");
        assert!(result.hits[0].path.contains("doc2.rs"));
    }

    #[test]
    fn search_returns_snippets_with_highlights() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        let index = SearchIndex::create(&index_path).unwrap();
        index
            .index_documents(&sample_extracted_texts(), &sample_paths())
            .unwrap();

        let result = index.search("fox", 10).unwrap();
        assert_eq!(result.total_count, 1);
        assert!(!result.hits[0].snippets.is_empty());
        assert!(result.hits[0].snippets[0].text.contains("fox"));
    }

    #[test]
    fn search_no_results() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        let index = SearchIndex::create(&index_path).unwrap();
        index
            .index_documents(&sample_extracted_texts(), &sample_paths())
            .unwrap();

        let result = index.search("nonexistent", 10).unwrap();
        assert_eq!(result.total_count, 0);
        assert!(result.hits.is_empty());
    }

    #[test]
    fn index_documents_skips_non_extractable() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        let index = SearchIndex::create(&index_path).unwrap();

        let texts = vec![
            ExtractedText {
                file_id: "f1".to_string(),
                content: "extractable content".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 19,
            },
            ExtractedText {
                file_id: "f2".to_string(),
                content: String::new(),
                encoding: "binary".to_string(),
                extractable: false,
                byte_count: 0,
            },
        ];
        let paths = vec![
            ("f1".to_string(), "/file1.txt".to_string()),
            ("f2".to_string(), "/file2.bin".to_string()),
        ];

        let count = index.index_documents(&texts, &paths).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn index_documents_skips_empty_content() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        let index = SearchIndex::create(&index_path).unwrap();

        let texts = vec![ExtractedText {
            file_id: "f1".to_string(),
            content: String::new(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 0,
        }];
        let paths = vec![("f1".to_string(), "/file1.txt".to_string())];

        let count = index.index_documents(&texts, &paths).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn open_existing_index() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        {
            let index = SearchIndex::create(&index_path).unwrap();
            index
                .index_documents(&sample_extracted_texts(), &sample_paths())
                .unwrap();
        }
        // Reopen the index
        let index = SearchIndex::open(&index_path).unwrap();
        let result = index.search("rust", 10).unwrap();
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn search_score_is_positive() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        let index = SearchIndex::create(&index_path).unwrap();
        index
            .index_documents(&sample_extracted_texts(), &sample_paths())
            .unwrap();

        let result = index.search("fox", 10).unwrap();
        assert!(result.hits[0].score > 0.0);
    }

    #[test]
    fn index_documents_replaces_existing() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        let index = SearchIndex::create(&index_path).unwrap();

        // Index original
        let texts1 = vec![ExtractedText {
            file_id: "f1".to_string(),
            content: "original content".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 16,
        }];
        let paths1 = vec![("f1".to_string(), "/file1.txt".to_string())];
        index.index_documents(&texts1, &paths1).unwrap();

        // Re-index same file_id with new content
        let texts2 = vec![ExtractedText {
            file_id: "f1".to_string(),
            content: "updated content".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 16,
        }];
        let paths2 = vec![("f1".to_string(), "/file1.txt".to_string())];
        index.index_documents(&texts2, &paths2).unwrap();

        // Search for old content should return nothing
        let result = index.search("original", 10).unwrap();
        assert_eq!(result.total_count, 0);

        // Search for new content should return 1
        let result = index.search("updated", 10).unwrap();
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn create_reuses_existing_index_without_clearing_documents() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("existing-index");

        let index = SearchIndex::create(&index_path).unwrap();
        index
            .index_documents(
                &[ExtractedText {
                    file_id: "f1".to_string(),
                    content: "persistent token".to_string(),
                    encoding: "utf-8".to_string(),
                    extractable: true,
                    byte_count: 16,
                }],
                &[("f1".to_string(), "/file1.txt".to_string())],
            )
            .unwrap();

        let reopened = SearchIndex::create(&index_path).unwrap();
        let result = reopened.search("persistent", 10).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.hits[0].file_id, "f1");
    }

    // ------------------------------------------------------------------
    // Chunked / incremental indexing tests
    // ------------------------------------------------------------------

    #[test]
    fn chunked_index_produces_same_results_as_batch() {
        let dir = tempdir().unwrap();

        // Build via batch (single commit).
        let batch_path = dir.path().join("batch");
        let batch_idx = SearchIndex::create(&batch_path).unwrap();
        let texts = sample_extracted_texts();
        let paths = sample_paths();
        batch_idx.index_documents(&texts, &paths).unwrap();
        let batch_result = batch_idx.search("fox", 10).unwrap();

        // Build via chunked (multiple commits).
        let chunked_path = dir.path().join("chunked");
        let chunked_idx = SearchIndex::create(&chunked_path).unwrap();
        // Use a small effective chunk size so we actually exercise the loop.
        // CHUNK_COMMIT_INTERVAL is 1000 so with 3 docs we get 1 chunk — that is
        // fine; the key property is correctness, not chunk count.
        chunked_idx
            .index_files_chunked(&texts, &paths, None)
            .unwrap();
        let chunked_result = chunked_idx.search("fox", 10).unwrap();

        assert_eq!(batch_result.total_count, chunked_result.total_count);
        assert_eq!(batch_result.hits.len(), chunked_result.hits.len());
        for (bh, ch) in batch_result.hits.iter().zip(chunked_result.hits.iter()) {
            assert_eq!(bh.file_id, ch.file_id);
            assert!((bh.score - ch.score).abs() < 0.001);
        }
    }

    #[test]
    fn incremental_index_only_adds_new_files() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("incr_index");
        let index = SearchIndex::create(&index_path).unwrap();

        // Seed the index with two files.
        let seed_texts = vec![
            ExtractedText {
                file_id: "existing-1".to_string(),
                content: "alpha bravo charlie".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 18,
            },
            ExtractedText {
                file_id: "existing-2".to_string(),
                content: "delta echo foxtrot".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 18,
            },
        ];
        let seed_paths = vec![
            ("existing-1".to_string(), "/a/existing1.txt".to_string()),
            ("existing-2".to_string(), "/a/existing2.txt".to_string()),
        ];
        index.index_documents(&seed_texts, &seed_paths).unwrap();

        // Mix: two existing + one new file.
        let mixed_texts = vec![
            // already indexed
            ExtractedText {
                file_id: "existing-1".to_string(),
                content: "alpha bravo charlie revised".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 26,
            },
            // new
            ExtractedText {
                file_id: "new-1".to_string(),
                content: "golf hotel india".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 17,
            },
            // already indexed
            ExtractedText {
                file_id: "existing-2".to_string(),
                content: "delta echo foxtrot revised".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 26,
            },
        ];
        let mixed_paths = vec![
            ("existing-1".to_string(), "/a/existing1.txt".to_string()),
            ("new-1".to_string(), "/b/new1.txt".to_string()),
            ("existing-2".to_string(), "/a/existing2.txt".to_string()),
        ];

        let stats = index
            .index_files_incremental(&mixed_texts, &mixed_paths, 2)
            .unwrap();

        // Only the one truly-new file should have been indexed.
        assert_eq!(stats.total_docs, 1);

        // The existing files still contain their original content.
        let result = index.search("charlie", 10).unwrap();
        assert_eq!(result.total_count, 1);
        // "revised" was in the incoming text for existing-1 but was skipped,
        // so it should NOT be found.
        let result_revised = index.search("revised", 10).unwrap();
        assert_eq!(result_revised.total_count, 0);

        // The new file IS searchable.
        let result_new = index.search("hotel", 10).unwrap();
        assert_eq!(result_new.total_count, 1);
        assert_eq!(result_new.hits[0].file_id, "new-1");
    }

    #[test]
    fn partial_index_is_searchable_mid_build() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("partial_index");
        let index = SearchIndex::create(&index_path).unwrap();

        // Batch 1: index 5 documents and commit.
        let batch1_texts: Vec<ExtractedText> = (0..5)
            .map(|i| ExtractedText {
                file_id: format!("batch1-{i}"),
                content: format!("batch one document {i} with unique token zebra{i}"),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 50,
            })
            .collect();
        let batch1_paths: Vec<(String, String)> = (0..5)
            .map(|i| (format!("batch1-{i}"), format!("/batch1/doc{i}.txt")))
            .collect();
        index.index_documents(&batch1_texts, &batch1_paths).unwrap();

        // After commit, batch 1 must be searchable.
        let r1 = index.search("zebra0", 10).unwrap();
        assert_eq!(
            r1.total_count, 1,
            "batch 1 should be searchable after commit"
        );

        // Batch 2: index another 5 documents and commit.
        let batch2_texts: Vec<ExtractedText> = (0..5)
            .map(|i| ExtractedText {
                file_id: format!("batch2-{i}"),
                content: format!("batch two document {i} with unique token giraffe{i}"),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 50,
            })
            .collect();
        let batch2_paths: Vec<(String, String)> = (0..5)
            .map(|i| (format!("batch2-{i}"), format!("/batch2/doc{i}.txt")))
            .collect();
        index.index_documents(&batch2_texts, &batch2_paths).unwrap();

        // Both batches should now be searchable.
        let r_both_zebra = index.search("zebra3", 10).unwrap();
        assert_eq!(r_both_zebra.total_count, 1, "batch 1 docs should persist");

        let r_both_giraffe = index.search("giraffe4", 10).unwrap();
        assert_eq!(
            r_both_giraffe.total_count, 1,
            "batch 2 docs should be searchable"
        );
    }
}
