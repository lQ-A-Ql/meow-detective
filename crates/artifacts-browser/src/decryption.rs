use crate::BrowserDecryptionStatus;

/// A caller-owned secret provider; format readers never own recovery keys.
pub trait BrowserSecretDecryptor {
    fn decrypt_value(
        &self,
        bytes: &[u8],
        cookie_domain: Option<&str>,
    ) -> (BrowserDecryptionStatus, Option<String>, Option<String>);
}
