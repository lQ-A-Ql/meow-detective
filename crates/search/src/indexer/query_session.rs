use tantivy::{
    collector::{
        sort_key::{SortBySimilarityScore, SortByString},
        Count, TopDocs,
    },
    query::{Query, QueryParser},
    schema::{Field, Value},
    DocAddress, Order, ReloadPolicy, Score, Searcher, TantivyDocument,
};

use super::search_after::SearchAfterCollector;
use super::tantivy_writer::{
    IndexError, Result, SearchHighlight, SearchHit, SearchIndex, SearchSnippet, StableTopDocs,
};

pub struct SearchQuerySession {
    searcher: Searcher,
    query: Box<dyn Query>,
    query_text: String,
    file_id_field: Field,
    path_field: Field,
    content_field: Field,
    index_generation: String,
    schema_version: u32,
    snapshot_opstamp: u64,
}

#[derive(Debug)]
pub struct SearchRankPage {
    pub hits: Vec<SearchRankedHit>,
    pub total_count: u64,
}

#[derive(Debug)]
pub struct SearchRankedHit {
    file_id: String,
    score: f64,
    address: DocAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAfterKey {
    pub score_bits: u32,
    pub file_id: String,
}

impl SearchRankedHit {
    pub fn file_id(&self) -> &str {
        &self.file_id
    }

    pub fn score(&self) -> f64 {
        self.score
    }

    pub fn after_key(&self) -> SearchAfterKey {
        SearchAfterKey {
            score_bits: (self.score as Score).to_bits(),
            file_id: self.file_id.clone(),
        }
    }
}

impl SearchIndex {
    pub fn query_session(&self, query_text: &str) -> Result<SearchQuerySession> {
        if !self.supports_stable_paging() {
            return Err(IndexError::Schema(
                "search index does not contain the stable file_id sort field".to_string(),
            ));
        }
        let snapshot_opstamp = self.snapshot_opstamp()?;
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let searcher = reader.searcher();
        if self.snapshot_opstamp()? != snapshot_opstamp {
            return Err(IndexError::Query(
                "search index changed while opening a query snapshot; retry the request"
                    .to_string(),
            ));
        }
        let file_id_field = required_field(self, "file_id")?;
        let path_field = required_field(self, "path")?;
        let content_field = required_field(self, "content")?;
        let parser = QueryParser::for_index(&self.index, vec![content_field]);
        let query = parser
            .parse_query(query_text)
            .or_else(|_| {
                let escaped = query_text.replace('"', "\\\"");
                parser.parse_query(&format!("\"{escaped}\""))
            })
            .map_err(|error| IndexError::Query(error.to_string()))?;
        Ok(SearchQuerySession {
            searcher,
            query,
            query_text: query_text.to_string(),
            file_id_field,
            path_field,
            content_field,
            index_generation: self.generation().to_string(),
            schema_version: self.schema_version(),
            snapshot_opstamp,
        })
    }
}

impl SearchQuerySession {
    pub fn index_generation(&self) -> &str {
        &self.index_generation
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn snapshot_opstamp(&self) -> u64 {
        self.snapshot_opstamp
    }

    pub fn rank_page(&self, offset: usize, limit: usize) -> Result<SearchRankPage> {
        if limit == 0 {
            let total_count = self.searcher.search(&self.query, &Count)?;
            return Ok(SearchRankPage {
                hits: Vec::new(),
                total_count: total_count as u64,
            });
        }
        let collector = TopDocs::with_limit(limit).and_offset(offset).order_by((
            (SortBySimilarityScore, Order::Desc),
            (SortByString::for_field("file_id"), Order::Asc),
        ));
        let (docs, total_count): (StableTopDocs, usize) =
            self.searcher.search(&self.query, &(collector, Count))?;
        let mut hits = Vec::with_capacity(docs.len());
        for ((score, file_id), address) in docs {
            let file_id = file_id.ok_or_else(missing_file_id_error)?;
            hits.push(SearchRankedHit {
                file_id,
                score: score as f64,
                address,
            });
        }
        Ok(SearchRankPage {
            hits,
            total_count: total_count as u64,
        })
    }

    pub fn rank_after(
        &self,
        after: Option<&SearchAfterKey>,
        limit: usize,
    ) -> Result<SearchRankPage> {
        if limit == 0 {
            let total_count = self.searcher.search(&self.query, &Count)?;
            return Ok(SearchRankPage {
                hits: Vec::new(),
                total_count: total_count as u64,
            });
        }
        let (docs, total_count) = self
            .searcher
            .search(
                &self.query,
                &(SearchAfterCollector::new(limit, after), Count),
            )
            .map_err(map_search_error)?;
        Ok(SearchRankPage {
            hits: docs
                .into_iter()
                .map(|candidate| SearchRankedHit {
                    file_id: candidate.file_id,
                    score: candidate.score as f64,
                    address: candidate.address,
                })
                .collect(),
            total_count: total_count as u64,
        })
    }

    pub fn materialize(&self, ranked: SearchRankedHit) -> Result<SearchHit> {
        let doc: TantivyDocument = self.searcher.doc(ranked.address)?;
        let stored_file_id = required_text_field(&doc, self.file_id_field, "file_id")?;
        if stored_file_id != ranked.file_id {
            return Err(IndexError::Schema(
                "ranked search hit does not match its stored file_id".to_string(),
            ));
        }
        let path = required_text_field(&doc, self.path_field, "path")?;
        let content = required_text_field(&doc, self.content_field, "content")?;
        let snippets = crate::highlighter::highlight(&content, &self.query_text)
            .into_iter()
            .map(|snippet| SearchSnippet {
                text: snippet.text,
                highlights: snippet
                    .highlights
                    .into_iter()
                    .map(|highlight| SearchHighlight {
                        start: highlight.start,
                        end: highlight.end,
                    })
                    .collect(),
            })
            .collect();
        Ok(SearchHit {
            file_id: ranked.file_id,
            path,
            score: ranked.score,
            snippets,
        })
    }
}

fn required_field(index: &SearchIndex, name: &str) -> Result<Field> {
    index
        .schema
        .get_field(name)
        .map_err(|_| IndexError::Schema(format!("missing {name} field")))
}

fn required_text_field(doc: &TantivyDocument, field: Field, name: &str) -> Result<String> {
    doc.get_first(field)
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| IndexError::Schema(format!("search document is missing stored {name}")))
}

fn missing_file_id_error() -> IndexError {
    IndexError::Schema("search document is missing the stable file_id sort value".to_string())
}

fn map_search_error(error: tantivy::TantivyError) -> IndexError {
    match error {
        tantivy::TantivyError::SchemaError(message) => IndexError::Schema(message),
        error => IndexError::Tantivy(error),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/indexer/query_session.rs"]
mod tests;
