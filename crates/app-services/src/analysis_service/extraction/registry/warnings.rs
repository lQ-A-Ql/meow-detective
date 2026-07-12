use std::collections::BTreeSet;

const MAX_REGISTRY_WARNINGS: usize = 64;

/// Prefix warning codes, deduplicate, cap, and redact absolute filesystem paths.
pub(super) fn govern_registry_warnings(path: &str, raw: Vec<String>) -> Vec<String> {
    let sanitized = sanitize_registry_path(path);
    let mut seen = BTreeSet::new();
    let mut governed = Vec::with_capacity(raw.len().min(MAX_REGISTRY_WARNINGS + 1));
    for message in raw {
        let code = warning_code_for(&message);
        let message = redact_registry_path(&message, path, &sanitized);
        let entry = format!("[{code}] {sanitized}: {message}");
        if seen.insert(entry.clone()) {
            governed.push(entry);
        }
    }
    if governed.len() > MAX_REGISTRY_WARNINGS {
        governed.truncate(MAX_REGISTRY_WARNINGS - 1);
        governed.push(format!(
            "[REG-CAP] {sanitized}: additional registry warnings suppressed"
        ));
    }
    governed
}

fn redact_registry_path(message: &str, path: &str, sanitized: &str) -> String {
    let redacted = message
        .replace(path, sanitized)
        .replace(&path.replace('\\', "/"), sanitized);
    redacted.replace(&path.replace('/', "\\"), sanitized)
}

fn sanitize_registry_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.contains(":/") || normalized.starts_with('/') {
        normalized
            .rsplit('/')
            .next()
            .unwrap_or(&normalized)
            .to_string()
    } else {
        normalized
    }
}

fn warning_code_for(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("txlog") || lower.contains("log1") || lower.contains("log2") {
        "REG-TXLOG"
    } else if lower.contains("deleted") || lower.contains("recovery") || lower.contains("free cell")
    {
        "REG-RECOVERY"
    } else if lower.contains("security") || lower.contains("lsa") || lower.contains("cached") {
        "REG-SEC"
    } else if lower.contains("sam") {
        "REG-SAM"
    } else if lower.contains("ntuser") || lower.contains("userassist") || lower.contains("run mru")
    {
        "REG-NTUSER"
    } else if lower.contains("usrclass") || lower.contains("shellbag") || lower.contains("muicache")
    {
        "REG-USRCLASS"
    } else if lower.contains("amcache") {
        "REG-AMCACHE"
    } else if lower.contains("software") {
        "REG-SOFTWARE"
    } else if lower.contains("system") {
        "REG-SYSTEM"
    } else {
        "REG-WARN"
    }
}
