//! Helpers shared by more than one email container format.

const BODY_PREVIEW_MAX_LEN: usize = 500;
const BODY_PREVIEW_MAX_LINES: usize = 8;

pub(super) fn build_body_preview(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(BODY_PREVIEW_MAX_LINES)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(BODY_PREVIEW_MAX_LEN)
        .collect()
}

pub(super) fn identify_mail_client(x_mailer: &str, source_path: &str) -> Option<String> {
    let haystack = format!("{} {}", x_mailer, source_path).to_ascii_lowercase();
    [
        ("mailmasterpc", "NetEase Mail Master"),
        ("网易邮箱大师", "NetEase Mail Master"),
        ("microsoft outlook", "Microsoft Outlook"),
        ("microsoft office outlook", "Microsoft Outlook"),
        ("outlook", "Microsoft Outlook"),
        ("foxmail", "Foxmail"),
        ("thunderbird", "Mozilla Thunderbird"),
    ]
    .into_iter()
    .find_map(|(needle, client)| haystack.contains(needle).then(|| client.to_string()))
}
