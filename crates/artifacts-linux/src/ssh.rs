use ssh_key::{HashAlg, PrivateKey, PublicKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKeyMetadata {
    pub algorithm: String,
    pub fingerprint: String,
    pub encrypted: bool,
    pub comment: String,
    pub configuration: Option<String>,
}

impl SshKeyMetadata {
    fn from_public(key: &PublicKey, encrypted: bool) -> Self {
        Self {
            algorithm: key.algorithm().to_string(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
            encrypted,
            comment: key.comment().to_string(),
            configuration: None,
        }
    }
}

pub fn parse_ssh_key_metadata(
    input: &[u8],
    public: bool,
) -> Result<SshKeyMetadata, ssh_key::Error> {
    if public {
        let text = std::str::from_utf8(input).map_err(|_| ssh_key::Error::FormatEncoding)?;
        PublicKey::from_openssh(text).map(|key| SshKeyMetadata::from_public(&key, false))
    } else {
        PrivateKey::from_openssh(input)
            .map(|key| SshKeyMetadata::from_public(key.public_key(), key.is_encrypted()))
    }
}

pub fn parse_authorized_keys_metadata(input: &str) -> Result<Vec<SshKeyMetadata>, ssh_key::Error> {
    ssh_key::AuthorizedKeys::new(input)
        .map(|entry| {
            entry.map(|entry| {
                let mut key = SshKeyMetadata::from_public(entry.public_key(), false);
                key.configuration = Some(entry.to_string());
                key
            })
        })
        .collect()
}

pub fn parse_known_hosts_metadata(input: &str) -> Result<Vec<SshKeyMetadata>, ssh_key::Error> {
    ssh_key::KnownHosts::new(input)
        .map(|entry| {
            entry.map(|entry| {
                let mut key = SshKeyMetadata::from_public(entry.public_key(), false);
                key.configuration = Some(entry.to_string());
                key
            })
        })
        .collect()
}
