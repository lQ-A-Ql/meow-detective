use super::*;

fn candidate(path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId(format!("file:{path}")),
        data_source_id: "source-1".to_string(),
        partition_index: Some(2),
        path: path.to_string(),
        size: 4096,
        content_identity: format!("identity:{path}"),
        evidence_kind: "browser".to_string(),
        parser: "browser.history".to_string(),
        category: "BrowserHistory".to_string(),
    }
}

#[test]
fn dpapi_preload_is_limited_to_chromium_secret_stores() {
    let history = candidate("Users/alice/AppData/Local/Google/Chrome/User Data/Default/History");
    let cookies =
        candidate("Users/alice/AppData/Local/Google/Chrome/User Data/Default/Network/Cookies");

    assert!(browser_roots(&[history]).is_empty());
    assert_eq!(browser_roots(&[cookies]).len(), 1);
}
