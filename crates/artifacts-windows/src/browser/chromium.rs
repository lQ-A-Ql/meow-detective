use crate::dpapi::ChromiumDecryptor;
pub use artifacts_browser::chromium::*;
use artifacts_browser::BrowserSecretDecryptor;

pub fn parse_chrome_cookies_with_decryptor(
    bytes: &[u8],
    browser: &str,
    profile: Option<&str>,
    decryptor: Option<&ChromiumDecryptor>,
) -> artifacts_browser::Result<Vec<BrowserCookie>> {
    artifacts_browser::parse_chrome_cookies_with_decryptor(
        bytes,
        browser,
        profile,
        decryptor.map(|value| value as &dyn BrowserSecretDecryptor),
    )
}

pub fn parse_chrome_passwords_with_decryptor(
    bytes: &[u8],
    browser: &str,
    profile: Option<&str>,
    decryptor: Option<&ChromiumDecryptor>,
) -> artifacts_browser::Result<Vec<BrowserPassword>> {
    artifacts_browser::parse_chrome_passwords_with_decryptor(
        bytes,
        browser,
        profile,
        decryptor.map(|value| value as &dyn BrowserSecretDecryptor),
    )
}

#[cfg(test)]
#[path = "../../tests/unit/chromium.rs"]
mod tests;
