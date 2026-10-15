use super::{
    file_query::{source_queries, FileSearchOptions},
    tantivy_writer::{Result, SearchIndex},
};
use tantivy::query::{EnableScoring, Weight};
use tantivy::snippet::SnippetGenerator;
use tantivy::{DocAddress, Searcher, TantivyDocument};
pub(super) struct FileMatchSources {
    fields: Vec<SourceField>,
}
struct SourceField {
    name: String,
    weight: Box<dyn Weight>,
    snippet: Option<SnippetGenerator>,
}
impl FileMatchSources {
    pub(super) fn create(
        index: &SearchIndex,
        searcher: &Searcher,
        options: &FileSearchOptions,
    ) -> Result<Self> {
        let mut fields = Vec::new();
        for (name, query) in source_queries(index, options)? {
            let weight = query.weight(EnableScoring::disabled_from_searcher(searcher))?;
            let field = match name.as_str() {
                "content" => Some("content"),
                "artifactField" => Some("artifact_text"),
                _ => None,
            };
            let snippet = field
                .map(|field| {
                    let field = index.schema.get_field(field).map_err(|error| {
                        super::tantivy_writer::IndexError::Schema(error.to_string())
                    })?;
                    let mut generator = SnippetGenerator::create(searcher, query.as_ref(), field)?;
                    generator.set_max_num_chars(600);
                    Ok::<_, super::tantivy_writer::IndexError>(generator)
                })
                .transpose()?;
            fields.push(SourceField {
                name,
                weight,
                snippet,
            });
        }
        Ok(Self { fields })
    }
    pub(super) fn materialize(
        &self,
        searcher: &Searcher,
        address: DocAddress,
        document: &TantivyDocument,
    ) -> Result<(Vec<String>, Option<String>)> {
        let mut sources = Vec::new();
        let mut snippet = None;
        for field in &self.fields {
            let mut scorer = field
                .weight
                .scorer(searcher.segment_reader(address.segment_ord), 1.0)?;
            // DocSet::seek only accepts a forward target. Absent terms begin
            // at TERMINATED, so check the scorer before seeking.
            if scorer.doc() <= address.doc_id && scorer.seek(address.doc_id) == address.doc_id {
                sources.push(field.name.clone());
                if snippet.is_none() {
                    if let Some(generator) = &field.snippet {
                        let text = generator.snippet_from_doc(document).fragment().to_owned();
                        if !text.is_empty() {
                            snippet = Some(text);
                        }
                    }
                }
            }
        }
        Ok((sources, snippet))
    }
}
