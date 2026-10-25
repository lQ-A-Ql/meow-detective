pub fn parse_firefox_history(bytes: &[u8]) -> crate::Result<Vec<crate::BrowserVisit>> {
    crate::history::collect_history(
        bytes,
        crate::history::BrowserFamily::Firefox,
        "Firefox",
        None,
    )
}
