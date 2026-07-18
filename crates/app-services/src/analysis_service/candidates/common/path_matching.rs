#[derive(Debug, Clone, Copy)]
pub(crate) enum EvidencePathPattern {
    Suffix(&'static str),
    Contains(&'static str),
    ContainsAndSuffix(&'static str, &'static str),
}

pub(crate) fn normalize_evidence_path(path: &str) -> String {
    let normalized = strip_synthetic_root_prefix(&path.replace('\\', "/")).to_ascii_lowercase();
    if normalized.starts_with('/') {
        normalized
    } else {
        format!("/{normalized}")
    }
}

fn strip_synthetic_root_prefix(path: &str) -> String {
    let mut path = path.trim().trim_start_matches('/').to_string();
    let had_partition_marker = if let Some(stripped) = strip_partition_marker_prefix(&path) {
        path = stripped.to_string();
        true
    } else {
        false
    };
    let components = path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();

    if components
        .first()
        .is_some_and(|component| is_linux_root_component(component))
        || !looks_like_synthetic_linux_prefix(&components, had_partition_marker)
    {
        return path;
    }

    let Some(index) = linux_root_start_index(&components) else {
        return path;
    };
    if index == 0 {
        path
    } else {
        components[index..].join("/")
    }
}

fn strip_partition_marker_prefix(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("[P")?;
    let (partition, after_partition) = rest.split_once(']')?;
    if partition.is_empty() || !partition.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let stripped = after_partition.trim_start_matches('/');
    (!stripped.is_empty()).then_some(stripped)
}

fn looks_like_synthetic_linux_prefix(components: &[&str], had_partition_marker: bool) -> bool {
    had_partition_marker
        || components
            .first()
            .is_some_and(|component| looks_like_partition_or_volume_root(component))
        || (components.len() >= 3
            && components[1].eq_ignore_ascii_case("root")
            && !is_linux_root_component(components[0]))
}

fn looks_like_partition_or_volume_root(component: &str) -> bool {
    let lower = component.to_ascii_lowercase();
    lower.starts_with("partition ") || lower.starts_with("volume")
}

fn linux_root_start_index(components: &[&str]) -> Option<usize> {
    for (index, component) in components.iter().enumerate() {
        if !is_linux_root_component(component) {
            continue;
        }
        if component.eq_ignore_ascii_case("root")
            && index > 0
            && components
                .get(index + 1)
                .is_some_and(|next| is_linux_root_component(next))
        {
            continue;
        }
        return Some(index);
    }
    None
}

fn is_linux_root_component(component: &str) -> bool {
    matches!(
        component.to_ascii_lowercase().as_str(),
        "bin"
            | "boot"
            | "dev"
            | "etc"
            | "home"
            | "lib"
            | "lib64"
            | "opt"
            | "root"
            | "run"
            | "sbin"
            | "srv"
            | "tmp"
            | "usr"
            | "var"
    )
}

pub(super) fn evidence_path_matches(path: &str, patterns: &[EvidencePathPattern]) -> bool {
    patterns.iter().any(|pattern| match pattern {
        EvidencePathPattern::Suffix(suffix) => path.ends_with(suffix),
        EvidencePathPattern::Contains(needle) => path.contains(needle),
        EvidencePathPattern::ContainsAndSuffix(needle, suffix) => {
            path.contains(needle) && path.ends_with(suffix)
        }
    })
}
