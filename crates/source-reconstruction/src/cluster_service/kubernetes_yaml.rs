use std::collections::BTreeMap;

use super::kubernetes_parser_error::{KubernetesParserError, Result};

const MAX_YAML_BYTES: usize = 8 * 1024 * 1024;
const MAX_YAML_DOCUMENTS: usize = 256;
const MAX_YAML_LINES: usize = 131_072;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YamlNode {
    Map(BTreeMap<String, YamlNode>),
    Seq(Vec<YamlNode>),
    Scalar(String),
    Null,
}

#[derive(Debug, Clone)]
struct YamlLine {
    number: usize,
    indent: usize,
    text: String,
}

pub(crate) fn parse_yaml_documents(input: &str) -> Result<Vec<YamlNode>> {
    if input.len() > MAX_YAML_BYTES {
        return Err(KubernetesParserError::Limit {
            kind: "yaml bytes",
            actual: input.len(),
            max: MAX_YAML_BYTES,
        });
    }
    let mut documents = Vec::new();
    let mut current = Vec::new();
    for (index, raw) in input.lines().enumerate() {
        let number = index + 1;
        let trimmed = raw.trim();
        if trimmed == "---" || trimmed == "..." {
            if !current.is_empty() {
                documents.push(parse_document(&current)?);
                current.clear();
            }
            if documents.len() >= MAX_YAML_DOCUMENTS {
                return Err(KubernetesParserError::Limit {
                    kind: "yaml documents",
                    actual: documents.len() + 1,
                    max: MAX_YAML_DOCUMENTS,
                });
            }
            continue;
        }
        let without_comment = strip_comment(raw);
        if without_comment.trim().is_empty() {
            continue;
        }
        let indent = without_comment.chars().take_while(|c| *c == ' ').count();
        if without_comment[..indent].contains('\t') {
            return Err(KubernetesParserError::InvalidYaml {
                line: number,
                reason: "tabs are not accepted for indentation".to_string(),
            });
        }
        current.push(YamlLine {
            number,
            indent,
            text: without_comment[indent..].trim_end().to_string(),
        });
        if current.len() > MAX_YAML_LINES {
            return Err(KubernetesParserError::Limit {
                kind: "yaml lines",
                actual: current.len(),
                max: MAX_YAML_LINES,
            });
        }
    }
    if !current.is_empty() {
        documents.push(parse_document(&current)?);
    }
    Ok(documents)
}

pub(crate) fn map_get<'a>(node: &'a YamlNode, key: &str) -> Option<&'a YamlNode> {
    match node {
        YamlNode::Map(map) => map.get(key),
        _ => None,
    }
}

pub(crate) fn map_string(node: &YamlNode, key: &str) -> Option<String> {
    match map_get(node, key) {
        Some(YamlNode::Scalar(value)) => Some(value.clone()),
        Some(YamlNode::Null) => None,
        _ => None,
    }
}

pub(crate) fn map_bool(node: &YamlNode, key: &str) -> Option<bool> {
    map_string(node, key).and_then(|value| match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

pub(crate) fn map_seq<'a>(node: &'a YamlNode, key: &str) -> &'a [YamlNode] {
    match map_get(node, key) {
        Some(YamlNode::Seq(values)) => values,
        _ => &[],
    }
}

fn parse_document(lines: &[YamlLine]) -> Result<YamlNode> {
    if lines.is_empty() {
        return Ok(YamlNode::Null);
    }
    let (node, next) = parse_block(lines, 0, lines[0].indent)?;
    if next != lines.len() {
        return Err(KubernetesParserError::InvalidYaml {
            line: lines[next].number,
            reason: "unexpected indentation or trailing content".to_string(),
        });
    }
    Ok(node)
}

fn parse_block(lines: &[YamlLine], start: usize, indent: usize) -> Result<(YamlNode, usize)> {
    if start >= lines.len() || lines[start].indent != indent {
        return Err(KubernetesParserError::InvalidYaml {
            line: lines.get(start).map_or(0, |line| line.number),
            reason: "expected an indented YAML block".to_string(),
        });
    }
    if lines[start].text == "-" || lines[start].text.starts_with("- ") {
        parse_sequence(lines, start, indent)
    } else {
        parse_mapping(lines, start, indent)
    }
}

fn parse_mapping(lines: &[YamlLine], mut index: usize, indent: usize) -> Result<(YamlNode, usize)> {
    let mut map = BTreeMap::new();
    while index < lines.len() && lines[index].indent == indent {
        if lines[index].text.starts_with('-') {
            break;
        }
        let line = &lines[index];
        let (key, rest) =
            split_mapping(&line.text).ok_or_else(|| KubernetesParserError::InvalidYaml {
                line: line.number,
                reason: "mapping entry must contain ':'".to_string(),
            })?;
        if key == "<<" || key.starts_with('&') || key.starts_with('*') {
            return Err(KubernetesParserError::InvalidYaml {
                line: line.number,
                reason: "anchors and merge keys are unsupported".to_string(),
            });
        }
        if map.contains_key(&key) {
            return Err(KubernetesParserError::InvalidYaml {
                line: line.number,
                reason: format!("duplicate mapping key '{key}'"),
            });
        }
        index += 1;
        let value = parse_value_or_child(lines, &mut index, indent, rest, line.number)?;
        map.insert(key, value);
    }
    Ok((YamlNode::Map(map), index))
}

fn parse_sequence(
    lines: &[YamlLine],
    mut index: usize,
    indent: usize,
) -> Result<(YamlNode, usize)> {
    let mut values = Vec::new();
    while index < lines.len() && lines[index].indent == indent {
        let line = &lines[index];
        if !line.text.starts_with('-') {
            break;
        }
        let rest = line.text[1..].trim_start();
        index += 1;
        if rest.is_empty() {
            let value = if index < lines.len() && lines[index].indent > indent {
                let child_indent = lines[index].indent;
                let (value, next) = parse_block(lines, index, child_indent)?;
                index = next;
                value
            } else {
                YamlNode::Null
            };
            values.push(value);
            continue;
        }
        if let Some((key, first_value)) = split_mapping(rest) {
            let mut map = BTreeMap::new();
            let value = parse_value_or_child(lines, &mut index, indent, first_value, line.number)?;
            map.insert(key, value);
            if index < lines.len() && lines[index].indent > indent {
                let child_indent = lines[index].indent;
                let (extra, next) = parse_block(lines, index, child_indent)?;
                index = next;
                let YamlNode::Map(extra_map) = extra else {
                    return Err(KubernetesParserError::InvalidYaml {
                        line: lines[index.saturating_sub(1)].number,
                        reason: "sequence mapping continuation must be a mapping".to_string(),
                    });
                };
                for (extra_key, extra_value) in extra_map {
                    if map.insert(extra_key.clone(), extra_value).is_some() {
                        return Err(KubernetesParserError::InvalidYaml {
                            line: line.number,
                            reason: format!("duplicate mapping key '{extra_key}'"),
                        });
                    }
                }
            }
            values.push(YamlNode::Map(map));
        } else {
            values.push(parse_scalar(rest, line.number)?);
        }
    }
    Ok((YamlNode::Seq(values), index))
}

fn parse_value_or_child(
    lines: &[YamlLine],
    index: &mut usize,
    parent_indent: usize,
    rest: &str,
    line_number: usize,
) -> Result<YamlNode> {
    if rest.is_empty() {
        if *index < lines.len()
            && (lines[*index].indent > parent_indent
                || (lines[*index].indent == parent_indent && lines[*index].text.starts_with('-')))
        {
            let child_indent = lines[*index].indent;
            let (value, next) = parse_block(lines, *index, child_indent)?;
            *index = next;
            Ok(value)
        } else {
            Ok(YamlNode::Null)
        }
    } else if rest == "|" || rest == ">" {
        let folded = rest == ">";
        let mut parts = Vec::new();
        while *index < lines.len() && lines[*index].indent > parent_indent {
            parts.push(lines[*index].text.clone());
            *index += 1;
        }
        let separator = if folded { " " } else { "\n" };
        Ok(YamlNode::Scalar(parts.join(separator)))
    } else {
        parse_scalar(rest, line_number)
    }
}

fn split_mapping(text: &str) -> Option<(String, &str)> {
    let mut quote = None;
    for (index, character) in text.char_indices() {
        match (quote, character) {
            (None, '\'' | '"') => quote = Some(character),
            (Some(current), value) if value == current => quote = None,
            (None, ':') => {
                let key = unquote(text[..index].trim());
                if key.is_empty() {
                    return None;
                }
                return Some((key, text[index + 1..].trim()));
            }
            _ => {}
        }
    }
    None
}

fn parse_scalar(value: &str, line: usize) -> Result<YamlNode> {
    let value = value.trim();
    if value.is_empty() || value == "~" || value.eq_ignore_ascii_case("null") {
        return Ok(YamlNode::Null);
    }
    if value.starts_with('&') || value.starts_with('*') || value.starts_with('!') {
        return Err(KubernetesParserError::InvalidYaml {
            line,
            reason: "anchors, aliases and tags are unsupported".to_string(),
        });
    }
    if (value.starts_with('[') || value.starts_with('{'))
        && serde_json::from_str::<serde_json::Value>(value).is_err()
    {
        return Err(KubernetesParserError::InvalidYaml {
            line,
            reason: "flow collections must use JSON-compatible syntax".to_string(),
        });
    }
    Ok(YamlNode::Scalar(unquote(value)))
}

fn unquote(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        if let Ok(decoded) = serde_json::from_str::<String>(value) {
            return decoded;
        }
    }
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        return value[1..value.len() - 1].replace("''", "'");
    }
    value.to_string()
}

fn strip_comment(value: &str) -> String {
    let mut quote = None;
    for (index, character) in value.char_indices() {
        match (quote, character) {
            (None, '\'' | '"') => quote = Some(character),
            (Some(current), value) if value == current => quote = None,
            (None, '#')
                if index == 0
                    || value[..index]
                        .chars()
                        .last()
                        .is_some_and(char::is_whitespace) =>
            {
                return value[..index].to_string()
            }
            _ => {}
        }
    }
    value.to_string()
}
