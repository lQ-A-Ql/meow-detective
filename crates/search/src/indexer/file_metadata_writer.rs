use tantivy::schema::{Field, Schema};
use tantivy::{IndexWriter, TantivyDocument, Term};

use super::file_query::normalize;
use super::tantivy_writer::{IndexError, Result, SearchIndex};

const INCREMENTAL_WRITER_MEMORY_BYTES: usize = 50_000_000;
const NEW_GENERATION_WRITER_MEMORY_BYTES: usize = 128_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchFileDocument {
    pub file_id: String,
    pub path: String,
    pub name: String,
    pub extension: String,
    pub entry_type: String,
    pub size: Option<u64>,
    pub modified_at: Option<i64>,
    pub deleted: bool,
    pub hidden: bool,
    pub system: bool,
    pub encrypted: bool,
}

pub struct SearchMetadataWriter {
    writer: IndexWriter,
    fields: MetadataFields,
    replace_existing: bool,
}

struct MetadataFields {
    file_id: Field,
    path: Field,
    name: Field,
    name_exact: Field,
    name_unigram: Field,
    name_bigram: Field,
    name_trigram: Field,
    path_exact: Field,
    sort_name: Field,
    sort_path: Field,
    sort_size: Field,
    sort_modified: Field,
    extension: Field,
    entry_type: Field,
    size: Field,
    modified_at: Field,
    deleted: Field,
    hidden: Field,
    system: Field,
    encrypted: Field,
}

impl SearchIndex {
    pub fn metadata_writer(&self) -> Result<SearchMetadataWriter> {
        self.metadata_writer_with_mode(true)
    }

    /// Open a writer for a freshly created generation.
    ///
    /// The generation is empty by construction, so deleting an existing
    /// document term before every insert only adds index work. Incremental
    /// callers must continue using [`Self::metadata_writer`].
    pub fn metadata_writer_for_new_generation(&self) -> Result<SearchMetadataWriter> {
        self.metadata_writer_with_mode(false)
    }

    fn metadata_writer_with_mode(&self, replace_existing: bool) -> Result<SearchMetadataWriter> {
        let memory_budget = if replace_existing {
            INCREMENTAL_WRITER_MEMORY_BYTES
        } else {
            NEW_GENERATION_WRITER_MEMORY_BYTES
        };
        Ok(SearchMetadataWriter {
            writer: self.index.writer(memory_budget)?,
            fields: MetadataFields::load(&self.schema)?,
            replace_existing,
        })
    }
}

impl SearchMetadataWriter {
    pub fn add_documents(&mut self, documents: &[SearchFileDocument]) -> Result<u64> {
        for document in documents {
            if self.replace_existing {
                self.writer.delete_term(Term::from_field_text(
                    self.fields.file_id,
                    &document.file_id,
                ));
            }
            self.writer.add_document(self.fields.document(document))?;
        }
        Ok(documents.len() as u64)
    }

    pub fn commit(mut self) -> Result<u64> {
        let opstamp = self.writer.commit()?;
        self.writer.wait_merging_threads()?;
        Ok(opstamp)
    }
}

impl MetadataFields {
    fn load(schema: &Schema) -> Result<Self> {
        let field = |name: &str| {
            schema
                .get_field(name)
                .map_err(|_| IndexError::Schema(format!("missing {name} field")))
        };
        Ok(Self {
            file_id: field("file_id")?,
            path: field("path")?,
            name: field("name")?,
            name_exact: field("name_exact")?,
            name_unigram: field("name_unigram")?,
            name_bigram: field("name_bigram")?,
            name_trigram: field("name_trigram")?,
            path_exact: field("path_exact")?,
            sort_name: field("sort_name")?,
            sort_path: field("sort_path")?,
            sort_size: field("sort_size")?,
            sort_modified: field("sort_modified")?,
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

    fn document(&self, source: &SearchFileDocument) -> TantivyDocument {
        let normalized_name = normalize(&source.name);
        let normalized_path = normalize(&source.path);
        let mut indexed = TantivyDocument::default();
        indexed.add_text(self.file_id, &source.file_id);
        indexed.add_text(self.path, &source.path);
        indexed.add_text(self.name, &source.name);
        self.add_searchable_name(&mut indexed, &normalized_name);
        indexed.add_text(self.path_exact, &normalized_path);
        indexed.add_text(self.sort_name, &normalized_name);
        indexed.add_text(self.sort_path, &normalized_path);
        indexed.add_text(self.sort_size, format!("{:020}", source.size.unwrap_or(0)));
        indexed.add_text(
            self.sort_modified,
            sortable_i64(source.modified_at.unwrap_or(i64::MIN)),
        );
        indexed.add_text(self.extension, normalize(&source.extension));
        indexed.add_text(self.entry_type, normalize(&source.entry_type));
        if let Some(size) = source.size {
            indexed.add_u64(self.size, size);
        }
        if let Some(modified_at) = source.modified_at {
            indexed.add_i64(self.modified_at, modified_at);
        }
        indexed.add_bool(self.deleted, source.deleted);
        indexed.add_bool(self.hidden, source.hidden);
        indexed.add_bool(self.system, source.system);
        indexed.add_bool(self.encrypted, source.encrypted);
        indexed
    }

    fn add_searchable_name(&self, document: &mut TantivyDocument, normalized: &str) {
        document.add_text(self.name_exact, normalized);
        document.add_text(self.name_unigram, normalized);
        document.add_text(self.name_bigram, normalized);
        document.add_text(self.name_trigram, normalized);
    }
}

fn sortable_i64(value: i64) -> String {
    format!("{:020}", (value as u64) ^ (1u64 << 63))
}
