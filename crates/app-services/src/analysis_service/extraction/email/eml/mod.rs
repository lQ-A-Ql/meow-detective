//! Single-message EML/EMLX parsing and extraction.

mod extract;
mod parser;

pub(crate) use extract::{extract_eml_candidate, extract_parsed_email};
pub(crate) use parser::parse_email_message;
