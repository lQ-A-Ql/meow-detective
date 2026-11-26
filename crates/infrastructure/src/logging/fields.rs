use regex::Regex;
use std::{collections::BTreeMap, fmt, sync::OnceLock};
use tracing::field::{Field, Visit};

#[derive(Clone, Default)]
pub(crate) struct CapturedFields {
    pub values: BTreeMap<String, String>,
    pub truncated: bool,
}

impl Visit for CapturedFields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert(field.name(), format_args!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field.name(), format_args!("{value}"));
    }
}

impl CapturedFields {
    fn insert(&mut self, name: &str, value: fmt::Arguments<'_>) {
        let lower = name.to_ascii_lowercase();
        if [
            "password",
            "secret",
            "token",
            "credential",
            "key_bytes",
            "plaintext",
            "payload",
        ]
        .iter()
        .any(|sensitive| lower.contains(sensitive))
        {
            return;
        }
        if self.values.len() >= 24 && !self.values.contains_key(name) {
            self.truncated = true;
            return;
        }
        let mut text = BoundedText {
            text: String::new(),
            limit: if name == "message" { 2048 } else { 512 },
            truncated: false,
        };
        let _ = fmt::write(&mut text, value);
        self.truncated |= text.truncated;
        self.values.insert(name.to_owned(), sanitize(&text.text));
    }
}

pub(crate) fn sanitize(value: &str) -> String {
    static HOST_PATH: OnceLock<Regex> = OnceLock::new();
    static SECRET: OnceLock<Regex> = OnceLock::new();
    let paths = HOST_PATH
        .get_or_init(|| Regex::new(r#"[A-Za-z]:[\\/][^\s,;"']+"#).expect("host path expression"));
    let secrets = SECRET.get_or_init(|| {
        Regex::new(r#"(?i)(password|secret|token|credential)\s*[=:]\s*[^\s,;]+"#)
            .expect("sensitive field expression")
    });
    secrets
        .replace_all(&paths.replace_all(value, "<host-path>"), "$1=<redacted>")
        .into_owned()
}

struct BoundedText {
    text: String,
    limit: usize,
    truncated: bool,
}

impl fmt::Write for BoundedText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for character in text.chars() {
            if self.text.len() + character.len_utf8() > self.limit {
                self.truncated = true;
                return Err(fmt::Error);
            }
            self.text.push(if character.is_control() {
                ' '
            } else {
                character
            });
        }
        Ok(())
    }
}
