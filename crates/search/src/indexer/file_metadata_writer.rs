use tantivy::schema::{Field, Schema};
use tantivy::{IndexWriter, TantivyDocument, Term};

use super::file_query::normalize;
use super::tantivy_writer::{IndexError, Result, SearchIndex};

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
    path_unigram: Field,
    path_bigram: Field,
    path_trigram: Field,
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
        Ok(SearchMetadataWriter {
            writer: self.index.writer(50_000_000)?,
            fields: MetadataFields::load(&self.schema)?,
        })
    }
}

impl SearchMetadataWriter {
    pub fn add_documents(&mut self, documents: &[SearchFileDocument]) -> Result<u64> {
        for document in documents {
            self.writer.delete_term(Term::from_field_text(
                self.fields.file_id,
                &document.file_id,
            ));
            self.writer.add_document(self.fields.document(document))?;
        }
        Ok(documents.len() as u64)
    }

    pub fn commit(mut self) -> Result<u64> {
        Ok(self.writer.commit()?)
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
            path_unigram: field("path_unigram")?,
            path_bigram: field("path_bigram")?,
            path_trigram: field("path_trigram")?,
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
        let mut indexed = TantivyDocument::default();
        indexed.add_text(self.file_id, &source.file_id);
        indexed.add_text(self.path, &source.path);
        indexed.add_text(self.name, &source.name);
        self.add_searchable_value(&mut indexed, &source.name, true);
        self.add_searchable_value(&mut indexed, &source.path, false);
        indexed.add_text(self.sort_name, normalize(&source.name));
        indexed.add_text(self.sort_path, normalize(&source.path));
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

    fn add_searchable_value(&self, document: &mut TantivyDocument, value: &str, is_name: bool) {
        let normalized = normalize(value);
        let (exact, unigram, bigram, trigram) = if is_name {
            (
                self.name_exact,
                self.name_unigram,
                self.name_bigram,
                self.name_trigram,
            )
        } else {
            (
                self.path_exact,
                self.path_unigram,
                self.path_bigram,
                self.path_trigram,
            )
        };
        document.add_text(exact, &normalized);
        document.add_text(unigram, &normalized);
        document.add_text(bigram, &normalized);
        document.add_text(trigram, &normalized);
    }
}

fn sortable_i64(value: i64) -> String {
    format!("{:020}", (value as u64) ^ (1u64 << 63))
}
