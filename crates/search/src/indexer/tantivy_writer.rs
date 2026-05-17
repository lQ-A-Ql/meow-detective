use crate::extractor::ExtractedText;
use std::path::Path;
use tantivy::{
    collector::TopDocs,
    doc,
    query::QueryParser,
    schema::{Schema, Value, STORED, STRING, TEXT},
    Index, IndexWriter, ReloadPolicy, TantivyDocument,
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
        let index = Index::create_in_dir(path, schema.clone())?;

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

        let file_id_field = self.schema.get_field("file_id").unwrap();
        let path_field = self.schema.get_field("path").unwrap();
        let content_field = self.schema.get_field("content").unwrap();
        let name_field = self.schema.get_field("name").unwrap();

        let path_map: std::collections::HashMap<&str, (&str, &str)> = paths
            .iter()
            .map(|(id, p)| (id.as_str(), (id.as_str(), p.as_str())))
            .collect();

        let mut count = 0u64;
        for text in texts {
            if !text.extractable || text.content.is_empty() {
                continue;
            }
            let (file_id, path) = path_map
                .get(text.file_id.as_str())
                .copied()
                .unwrap_or((&text.file_id, ""));

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

    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        let searcher = reader.searcher();

        let content_field = self.schema.get_field("content").unwrap();
        let file_id_field = self.schema.get_field("file_id").unwrap();
        let path_field = self.schema.get_field("path").unwrap();

        let query_parser = QueryParser::for_index(&self.index, vec![content_field]);

        let query = query_parser
            .parse_query(query_str)
            .or_else(|_| {
                let escaped = query_str.replace('"', "\\\"");
                let phrase = format!("\"{}\"", escaped);
                query_parser.parse_query(&phrase)
            })
            .map_err(|e| IndexError::Query(e.to_string()))?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

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

        Ok(hits)
    }
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
