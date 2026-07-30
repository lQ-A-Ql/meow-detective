use tantivy::query::{AllQuery, BooleanQuery, Occur, PhraseQuery, Query, RegexQuery, TermQuery};
use tantivy::schema::{Field, IndexRecordOption};
use tantivy::Term;
use unicode_normalization::UnicodeNormalization;

use super::tantivy_writer::{IndexError, Result, SearchIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEntryTypeFilter {
    Any,
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSearchSortField {
    Name,
    Path,
    Size,
    ModifiedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSearchSortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchOptions {
    pub query: String,
    pub match_path: bool,
    pub entry_type: FileEntryTypeFilter,
    pub extensions: Vec<String>,
    pub sort_field: FileSearchSortField,
    pub sort_direction: FileSearchSortDirection,
}

impl Default for FileSearchOptions {
    fn default() -> Self {
        Self {
            query: String::new(),
            match_path: false,
            entry_type: FileEntryTypeFilter::Any,
            extensions: Vec::new(),
            sort_field: FileSearchSortField::Name,
            sort_direction: FileSearchSortDirection::Asc,
        }
    }
}

pub(super) fn compile_file_query(
    index: &SearchIndex,
    options: &FileSearchOptions,
) -> Result<Box<dyn Query>> {
    let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
    for term in split_query_terms(&options.query) {
        let name_query = substring_query(index, "name", &term)?;
        if options.match_path {
            let path_query = substring_query(index, "path", &term)?;
            clauses.push((
                Occur::Must,
                Box::new(BooleanQuery::new(vec![
                    (Occur::Should, name_query),
                    (Occur::Should, path_query),
                ])),
            ));
        } else {
            clauses.push((Occur::Must, name_query));
        }
    }

    if options.entry_type != FileEntryTypeFilter::Any {
        let field = required_field(index, "entry_type")?;
        let value = match options.entry_type {
            FileEntryTypeFilter::File => "file",
            FileEntryTypeFilter::Directory => "directory",
            FileEntryTypeFilter::Any => unreachable!(),
        };
        clauses.push((Occur::Must, exact_term(field, value)));
    }

    let extensions = options
        .extensions
        .iter()
        .map(|extension| normalize(extension.trim_start_matches('.')))
        .filter(|extension| !extension.is_empty())
        .collect::<Vec<_>>();
    if !extensions.is_empty() {
        let field = required_field(index, "extension")?;
        let extension_queries = extensions
            .iter()
            .map(|extension| (Occur::Should, exact_term(field, extension)))
            .collect();
        clauses.push((Occur::Must, Box::new(BooleanQuery::new(extension_queries))));
    }

    if clauses.is_empty() {
        Ok(Box::new(AllQuery))
    } else {
        Ok(Box::new(BooleanQuery::new(clauses)))
    }
}

fn substring_query(index: &SearchIndex, prefix: &str, raw: &str) -> Result<Box<dyn Query>> {
    let normalized = normalize(raw);
    if normalized.is_empty() {
        return Ok(Box::new(AllQuery));
    }
    if normalized.contains(['*', '?']) {
        let field = required_field(index, &format!("{prefix}_exact"))?;
        return RegexQuery::from_pattern(&glob_pattern(&normalized), field)
            .map(|query| Box::new(query) as Box<dyn Query>)
            .map_err(|error| IndexError::Query(error.to_string()));
    }

    let chars = normalized.chars().collect::<Vec<_>>();
    match chars.len() {
        1 => {
            let field = required_field(index, &format!("{prefix}_unigram"))?;
            Ok(exact_term(field, &normalized))
        }
        2 => {
            let field = required_field(index, &format!("{prefix}_bigram"))?;
            Ok(exact_term(field, &normalized))
        }
        _ => {
            let field = required_field(index, &format!("{prefix}_trigram"))?;
            let terms = chars
                .windows(3)
                .map(|window| Term::from_field_text(field, &window.iter().collect::<String>()))
                .collect::<Vec<_>>();
            if let [term] = terms.as_slice() {
                return Ok(Box::new(TermQuery::new(
                    term.clone(),
                    IndexRecordOption::WithFreqsAndPositions,
                )));
            }
            Ok(Box::new(PhraseQuery::new(terms)))
        }
    }
}

fn exact_term(field: Field, value: &str) -> Box<dyn Query> {
    Box::new(TermQuery::new(
        Term::from_field_text(field, value),
        IndexRecordOption::Basic,
    ))
}

fn split_query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in query.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                if !quoted && !current.trim().is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
            }
            character if character.is_whitespace() && !quoted => {
                if !current.trim().is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        terms.push(current);
    }
    terms
}

fn glob_pattern(value: &str) -> String {
    let mut pattern = String::new();
    for character in value.chars() {
        match character {
            '*' => pattern.push_str(".*"),
            '?' => pattern.push('.'),
            '.' | '+' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\' => {
                pattern.push('\\');
                pattern.push(character);
            }
            _ => pattern.push(character),
        }
    }
    pattern
}

pub fn normalize(value: &str) -> String {
    value.nfkc().flat_map(char::to_lowercase).collect()
}

fn required_field(index: &SearchIndex, name: &str) -> Result<Field> {
    index
        .schema
        .get_field(name)
        .map_err(|_| IndexError::Schema(format!("missing {name} field")))
}
