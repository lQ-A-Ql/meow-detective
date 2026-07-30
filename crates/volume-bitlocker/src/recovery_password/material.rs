use zeroize::Zeroizing;

use crate::bytes::le_u16;

use super::error::RecoveryPasswordRecoveryError;

const KEY_DATUM_LEN: usize = 28;
const KEY_DATUM_HEADER_LEN: usize = 12;
const ENTRY_TYPE_NONE: u16 = 0;
const VALUE_TYPE_KEY: u16 = 1;
const KEY_DATUM_VERSION: u16 = 0;
/// Algorithm field of the numerical-password key datum nested inside the
/// recovery protector's stretch key (distinct from the 0x2xxx volume cipher
/// codes). Validated against the private Liu Yang sample; it is not an AES
/// method identifier.
const RECOVERY_KEY_ALGORITHM: u16 = 0x1000;

pub(super) struct RecoveryPasswordMaterial {
    bytes: Zeroizing<[u8; 16]>,
}

impl RecoveryPasswordMaterial {
    pub(super) fn parse(plaintext: &[u8]) -> Result<Self, RecoveryPasswordRecoveryError> {
        if plaintext.len() != KEY_DATUM_LEN {
            return invalid("decrypted datum must be exactly 28 bytes");
        }
        if le_u16(plaintext, 0) as usize != KEY_DATUM_LEN
            || le_u16(plaintext, 2) != ENTRY_TYPE_NONE
            || le_u16(plaintext, 4) != VALUE_TYPE_KEY
            || le_u16(plaintext, 6) != KEY_DATUM_VERSION
            || le_u16(plaintext, 8) != RECOVERY_KEY_ALGORITHM
            || le_u16(plaintext, 10) != 0
        {
            return invalid("decrypted datum header is not a numerical-password key datum");
        }
        let mut bytes = Zeroizing::new([0u8; 16]);
        bytes.copy_from_slice(&plaintext[KEY_DATUM_HEADER_LEN..]);
        Ok(Self { bytes })
    }

    pub(super) fn expose_for_formatting(&self) -> &[u8; 16] {
        &self.bytes
    }
}

fn invalid<T>(reason: &'static str) -> Result<T, RecoveryPasswordRecoveryError> {
    Err(RecoveryPasswordRecoveryError::InvalidMaterial { reason })
}
