impl artifacts_browser::BrowserSecretDecryptor for super::ChromiumDecryptor {
    fn decrypt_value(
        &self,
        bytes: &[u8],
        cookie_domain: Option<&str>,
    ) -> (
        artifacts_browser::BrowserDecryptionStatus,
        Option<String>,
        Option<String>,
    ) {
        super::ChromiumDecryptor::decrypt_value(self, bytes, cookie_domain)
    }
}
