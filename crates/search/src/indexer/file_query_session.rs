use tantivy::collector::Count;
use tantivy::schema::{Field, Value};
use tantivy::{DocAddress, ReloadPolicy, Searcher, TantivyDocument};

use super::file_query::{
    compile_file_query, FileSearchOptions, FileSearchSortDirection, FileSearchSortField,
};
use super::file_search_after::FileSearchAfterCollector;
use super::tantivy_writer::{IndexError, Result, SearchIndex};

pub struct FileSearchQuerySession {
    searcher: Searcher,
    query: Box<dyn tantivy::query::Query>,
    fields: FileResultFields,
    sort_field: String,
    sort_direction: FileSearchSortDirection,
    index_generation: String,
    schema_version: u32,
    snapshot_opstamp: u64,
}

#[derive(Debug)]
pub struct FileSearchRankPage {
    pub hits: Vec<FileSearchRankedHit>,
    pub total_count: u64,
}

#[derive(Debug)]
pub struct FileSearchRankedHit {
    file_id: String,
    sort_value: String,
    address: DocAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchAfterKey {
    pub sort_value: String,
    pub file_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchHit {
    pub file_id: String,
    pub name: String,
    pub path: String,
    pub extension: String,
    pub entry_type: String,
    pub size: Option<u64>,
    pub modified_at: Option<i64>,
    pub deleted: bool,
    pub hidden: bool,
    pub system: bool,
    pub encrypted: bool,
}

struct FileResultFields {
    file_id: Field,
    name: Field,
    path: Field,
    extension: Field,
    entry_type: Field,
    size: Field,
    modified_at: Field,
    deleted: Field,
    hidden: Field,
    system: Field,
    encrypted: Field,
}

impl FileSearchRankedHit {
    pub fn file_id(&self) -> &str {
        &self.file_id
    }

    pub fn sort_value(&self) -> &str {
        &self.sort_value
    }

    pub fn after_key(&self) -> FileSearchAfterKey {
        FileSearchAfterKey {
            sort_value: self.sort_value.clone(),
            file_id: self.file_id.clone(),
        }
    }
}

impl SearchIndex {
    pub fn file_query_session(
        &self,
        options: &FileSearchOptions,
    ) -> Result<FileSearchQuerySession> {
        self.validate_file_search_schema()?;
        let snapshot_opstamp = self.snapshot_opstamp()?;
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let searcher = reader.searcher();
        if self.snapshot_opstamp()? != snapshot_opstamp {
            return Err(IndexError::Query(
                "search index changed while opening a file query snapshot".to_string(),
            ));
        }
        Ok(FileSearchQuerySession {
            searcher,
            query: compile_file_query(self, options)?,
            fields: FileResultFields::load(self)?,
            sort_field: sort_field_name(options.sort_field).to_string(),
            sort_direction: options.sort_direction,
            index_generation: self.generation().to_string(),
            schema_version: self.schema_version(),
            snapshot_opstamp,
        })
    }

    pub fn validate_file_search_schema(&self) -> Result<()> {
        for field in [
            "file_id",
            "sort_name",
            "sort_path",
            "sort_size",
            "sort_modified",
        ] {
            self.validate_text_field(field, true, true, true)?;
        }
        for field in [
            "name_exact",
            "name_unigram",
            "name_bigram",
            "name_trigram",
            "path_exact",
            "path_unigram",
            "path_bigram",
            "path_trigram",
            "extension",
            "entry_type",
        ] {
            self.validate_text_field(field, true, false, false)?;
        }
        Ok(())
    }
}

impl FileSearchQuerySession {
    pub fn index_generation(&self) -> &str {
        &self.index_generation
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn snapshot_opstamp(&self) -> u64 {
        self.snapshot_opstamp
    }

    pub fn rank_after(
        &self,
        after: Option<&FileSearchAfterKey>,
        limit: usize,
    ) -> Result<FileSearchRankPage> {
        if limit == 0 {
            return Ok(FileSearchRankPage {
                hits: Vec::new(),
                total_count: self.searcher.search(&self.query, &Count)? as u64,
            });
        }
        let (candidates, total_count) = self.searcher.search(
            &self.query,
            &(
                FileSearchAfterCollector::new(limit, after, &self.sort_field, self.sort_direction),
                Count,
            ),
        )?;
        Ok(FileSearchRankPage {
            hits: candidates
                .into_iter()
                .map(|candidate| FileSearchRankedHit {
                    file_id: candidate.file_id,
                    sort_value: candidate.sort_value,
                    address: candidate.address,
                })
                .collect(),
            total_count: total_count as u64,
        })
    }

    pub fn materialize(&self, ranked: FileSearchRankedHit) -> Result<FileSearchHit> {
        let document: TantivyDocument = self.searcher.doc(ranked.address)?;
        let file_id = required_text(&document, self.fields.file_id, "file_id")?;
        if file_id != ranked.file_id {
            return Err(IndexError::Schema(
                "ranked file search hit does not match stored file_id".to_string(),
            ));
        }
        Ok(FileSearchHit {
            file_id,
            name: required_text(&document, self.fields.name, "name")?,
            path: required_text(&document, self.fields.path, "path")?,
            extension: required_text(&document, self.fields.extension, "extension")?,
            entry_type: required_text(&document, self.fields.entry_type, "entry_type")?,
            size: optional_u64(&document, self.fields.size),
            modified_at: optional_i64(&document, self.fields.modified_at),
            deleted: optional_bool(&document, self.fields.deleted),
            hidden: optional_bool(&document, self.fields.hidden),
            system: optional_bool(&document, self.fields.system),
            encrypted: optional_bool(&document, self.fields.encrypted),
        })
    }
}

impl FileResultFields {
    fn load(index: &SearchIndex) -> Result<Self> {
        let field = |name: &str| {
            index
                .schema
                .get_field(name)
                .map_err(|_| IndexError::Schema(format!("missing {name} field")))
        };
        Ok(Self {
            file_id: field("file_id")?,
            name: field("name")?,
            path: field("path")?,
            extension: field("extension")?,
            entry_type: field("entry_type")?,
            size: field("size")?,
            modified_at: field("modified_at")?,
            deleted: field("deleted")?,
            hidden: field("hidden")?,
            system: field("system")?,
            encrypted: field("encrypted")?,
        })
    }
}

fn sort_field_name(field: FileSearchSortField) -> &'static str {
    match field {
        FileSearchSortField::Name => "sort_name",
        FileSearchSortField::Path => "sort_path",
        FileSearchSortField::Size => "sort_size",
        FileSearchSortField::ModifiedAt => "sort_modified",
    }
}

fn required_text(document: &TantivyDocument, field: Field, name: &str) -> Result<String> {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| IndexError::Schema(format!("missing stored {name}")))
}

fn optional_u64(document: &TantivyDocument, field: Field) -> Option<u64> {
    document.get_first(field).and_then(|value| value.as_u64())
}

fn optional_i64(document: &TantivyDocument, field: Field) -> Option<i64> {
    document.get_first(field).and_then(|value| value.as_i64())
}

fn optional_bool(document: &TantivyDocument, field: Field) -> bool {
    document
        .get_first(field)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}
