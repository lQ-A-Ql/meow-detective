use crate::{UpdateError, MAX_ARCHIVE_BYTES};
use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

pub fn hash_file(path: &Path) -> Result<String, UpdateError> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn verify_signature(path: &Path, signature: &str, public_key: &str) -> Result<(), UpdateError> {
    let decode = |text: &str| -> Result<String, UpdateError> {
        String::from_utf8(
            STANDARD
                .decode(text.trim())
                .map_err(|_| UpdateError::Signature)?,
        )
        .map_err(|_| UpdateError::Signature)
    };
    if std::fs::metadata(path)?.len() > MAX_ARCHIVE_BYTES {
        return Err(UpdateError::Invalid("archive exceeds size limit"));
    }
    let key = PublicKey::decode(&decode(public_key)?).map_err(|_| UpdateError::Signature)?;
    let sig = Signature::decode(&decode(signature)?).map_err(|_| UpdateError::Signature)?;
    let bytes = std::fs::read(path)?;
    key.verify(&bytes, &sig, true)
        .map_err(|_| UpdateError::Signature)
}
