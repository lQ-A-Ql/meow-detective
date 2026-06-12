use crate::extractor::ExtractedText;
use std::path::Path;
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

    pub fn search(&self, query_str: &str, limit: usize) -> Result<SearchResult> {
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

        let (top_docs, total_count) = searcher.search(
            &query,
            &(TopDocs::with_limit(limit).order_by_score(), Count),
        )?;

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
    fn search_respects_limit() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("test_index");
        let index = SearchIndex::create(&index_path).unwrap();

        let texts: Vec<ExtractedText> = (0..20)
            .map(|i| ExtractedText {
                file_id: format!("f{i}"),
                content: format!("document number {i} with test content"),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 50,
            })
            .collect();
        let paths: Vec<(String, String)> = (0..20)
            .map(|i| (format!("f{i}"), format!("/docs/doc{i}.txt")))
            .collect();

        index.index_documents(&texts, &paths).unwrap();
        let result = index.search("test", 5).unwrap();
        assert!(result.hits.len() <= 5);
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
}
