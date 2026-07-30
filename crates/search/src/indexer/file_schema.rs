use tantivy::schema::{
    IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST, STORED, STRING, TEXT,
};
use tantivy::tokenizer::{BoxTokenStream, NgramTokenizer, TokenStream, Tokenizer};
use tantivy::Index;

use super::tantivy_writer::{IndexError, Result};

pub(super) fn build_search_schema() -> Schema {
    let mut schema = Schema::builder();
    schema.add_text_field("file_id", STRING | STORED | FAST);
    schema.add_text_field("path", TEXT | STORED);
    schema.add_text_field("content", TEXT | STORED);
    schema.add_text_field("name", TEXT | STORED);
    schema.add_text_field("name_exact", STRING);
    schema.add_text_field("name_unigram", ngram_options("file_unigram"));
    schema.add_text_field("name_bigram", ngram_options("file_bigram"));
    schema.add_text_field("name_trigram", ngram_options("file_trigram"));
    schema.add_text_field("path_exact", STRING);
    schema.add_text_field("path_unigram", ngram_options("file_unigram"));
    schema.add_text_field("path_bigram", ngram_options("file_bigram"));
    schema.add_text_field("path_trigram", ngram_options("file_trigram"));
    schema.add_text_field("sort_name", STRING | STORED | FAST);
    schema.add_text_field("sort_path", STRING | STORED | FAST);
    schema.add_text_field("sort_size", STRING | STORED | FAST);
    schema.add_text_field("sort_modified", STRING | STORED | FAST);
    schema.add_text_field("extension", STRING | STORED);
    schema.add_text_field("entry_type", STRING | STORED);
    schema.add_u64_field("size", STORED | FAST);
    schema.add_i64_field("modified_at", STORED | FAST);
    schema.add_bool_field("deleted", STORED);
    schema.add_bool_field("hidden", STORED);
    schema.add_bool_field("system", STORED);
    schema.add_bool_field("encrypted", STORED);
    schema.build()
}

pub(super) fn register_file_tokenizers(index: &Index) -> Result<()> {
    for (name, width) in [
        ("file_unigram", 1usize),
        ("file_bigram", 2usize),
        ("file_trigram", 3usize),
    ] {
        let tokenizer = PositionedNgramTokenizer::new(width)
            .map_err(|error| IndexError::Schema(format!("invalid {name} tokenizer: {error}")))?;
        index.tokenizers().register(name, tokenizer);
    }
    Ok(())
}

fn ngram_options(tokenizer: &str) -> TextOptions {
    TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(tokenizer)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    )
}

#[derive(Clone)]
struct PositionedNgramTokenizer {
    inner: NgramTokenizer,
}

struct PositionedNgramTokenStream<'a> {
    inner: BoxTokenStream<'a>,
    next_position: usize,
}

impl PositionedNgramTokenizer {
    fn new(width: usize) -> Result<Self> {
        let inner = NgramTokenizer::new(width, width, false)
            .map_err(|error| IndexError::Schema(format!("invalid n-gram tokenizer: {error}")))?;
        Ok(Self { inner })
    }
}

impl Tokenizer for PositionedNgramTokenizer {
    type TokenStream<'a> = PositionedNgramTokenStream<'a>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        PositionedNgramTokenStream {
            inner: BoxTokenStream::new(self.inner.token_stream(text)),
            next_position: 0,
        }
    }
}

impl TokenStream for PositionedNgramTokenStream<'_> {
    fn advance(&mut self) -> bool {
        if !self.inner.advance() {
            return false;
        }
        self.inner.token_mut().position = self.next_position;
        self.next_position = self.next_position.saturating_add(1);
        true
    }

    fn token(&self) -> &tantivy::tokenizer::Token {
        self.inner.token()
    }

    fn token_mut(&mut self) -> &mut tantivy::tokenizer::Token {
        self.inner.token_mut()
    }
}
