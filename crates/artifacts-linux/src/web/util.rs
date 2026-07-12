pub(super) fn strip_inline_comment(line: &str) -> String {
    line.split('#').next().unwrap_or_default().to_string()
}

pub(super) fn brace_delta(line: &str) -> i32 {
    line.chars().filter(|ch| *ch == '{').count() as i32
        - line.chars().filter(|ch| *ch == '}').count() as i32
}

pub(super) fn push_tokens(target: &mut Vec<String>, value: &str) {
    for token in value.split_whitespace() {
        push_clean(target, token);
    }
}

pub(super) fn push_first_token(target: &mut Vec<String>, value: &str) {
    if let Some(token) = value.split_whitespace().next() {
        push_clean(target, token);
    }
}

fn push_clean(target: &mut Vec<String>, token: &str) {
    let cleaned = token
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches(';');
    if !cleaned.is_empty() && cleaned != "off" && !target.iter().any(|value| value == cleaned) {
        target.push(cleaned.to_string());
    }
}

pub(super) fn virtual_host_listen(line: &str) -> Vec<String> {
    let inner = line
        .trim_start_matches("<VirtualHost")
        .trim_end_matches('>')
        .trim();
    inner.split_whitespace().map(str::to_string).collect()
}

pub(super) fn extract_quoted(input: &str) -> Option<(String, &str)> {
    let start = input.find('"')?;
    let rest = &input[start + 1..];
    let end = rest.find('"')?;
    Some((rest[..end].to_string(), &rest[end + 1..]))
}

pub(super) fn dash_to_none(value: String) -> Option<String> {
    (!value.is_empty() && value != "-").then_some(value)
}
