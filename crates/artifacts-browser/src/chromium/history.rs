pub fn parse_chrome_history(
    bytes: &[u8],
    browser: &str,
    profile: Option<&str>,
) -> crate::Result<Vec<super::BrowserVisit>> {
    crate::history::collect_history(
        bytes,
        crate::history::BrowserFamily::Chromium,
        browser,
        profile,
    )
}
