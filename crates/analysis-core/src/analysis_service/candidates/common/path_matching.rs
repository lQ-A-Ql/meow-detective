pub(crate) use infrastructure::paths::evidence_relative_path;

#[derive(Debug, Clone, Copy)]
pub(crate) enum EvidencePathPattern {
    Suffix(&'static str),
    Contains(&'static str),
    ContainsAndSuffix(&'static str, &'static str),
    ContainsBoth(&'static str, &'static str),
    FoxmailBox,
    FoxmailIndex,
    FoxmailAccount,
}

pub fn normalize_evidence_path(path: &str) -> String {
    evidence_relative_path(path).to_ascii_lowercase()
}

pub(crate) fn evidence_path_matches(path: &str, patterns: &[EvidencePathPattern]) -> bool {
    patterns.iter().any(|pattern| match pattern {
        EvidencePathPattern::Suffix(suffix) => path.ends_with(suffix),
        EvidencePathPattern::Contains(needle) => path.contains(needle),
        EvidencePathPattern::ContainsAndSuffix(needle, suffix) => {
            path.contains(needle) && path.ends_with(suffix)
        }
        EvidencePathPattern::ContainsBoth(first, second) => {
            path.contains(first) && path.contains(second)
        }
        EvidencePathPattern::FoxmailBox => is_foxmail_box_path(path),
        EvidencePathPattern::FoxmailIndex => is_foxmail_index_path(path),
        EvidencePathPattern::FoxmailAccount => is_foxmail_account_path(path),
    })
}

fn is_foxmail_box_path(path: &str) -> bool {
    if !path.ends_with(".box") {
        return false;
    }
    let foxmail_directory = path
        .split('/')
        .any(|component| component.starts_with("foxmail"));
    let known_mailbox_name = path.rsplit('/').next().is_some_and(|name| {
        matches!(
            name,
            "in.box"
                | "out.box"
                | "send.box"
                | "sent.box"
                | "trash.box"
                | "deleted.box"
                | "draft.box"
                | "spam.box"
        )
    });
    foxmail_directory
        || path.contains("/data/mails/")
        || (path.contains("/storage/") && path.contains("/mails/"))
        || (path.contains("/mail/") && known_mailbox_name)
}

fn is_foxmail_index_path(path: &str) -> bool {
    if !path.ends_with("/mails/index") {
        return false;
    }
    path.split('/')
        .any(|component| component.starts_with("foxmail"))
        || (path.contains("/storage/") && path.contains("/mails/index"))
}

fn is_foxmail_account_path(path: &str) -> bool {
    if !path.ends_with("/accounts/account.rec0") {
        return false;
    }
    path.split('/')
        .any(|component| component.starts_with("foxmail"))
        || path.contains("/storage/")
}
