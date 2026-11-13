use domain::FileEncryptionStatus;
use sha2::{Digest, Sha256};

pub(crate) fn candidate_content_identity(
    file_id: &str,
    data_source_id: &str,
    partition_index: Option<usize>,
    path: &str,
    size: u64,
    encryption_status: FileEncryptionStatus,
    metadata: [Option<String>; 5],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"analysis-candidate-content-v1");
    for value in [file_id, data_source_id, path] {
        update_identity_field(&mut hasher, value.as_bytes());
    }
    hasher.update(size.to_le_bytes());
    match partition_index {
        Some(partition_index) => {
            hasher.update([1]);
            hasher.update((partition_index as u64).to_le_bytes());
        }
        None => hasher.update([0]),
    }
    match encryption_status {
        FileEncryptionStatus::Unknown => hasher.update(b"analysis-candidate-encryption-unknown"),
        FileEncryptionStatus::Clear => hasher.update(b"analysis-candidate-content-clear"),
        FileEncryptionStatus::Encrypted => hasher.update(b"analysis-candidate-efs-encrypted"),
    }
    for value in metadata {
        match value {
            Some(value) => {
                hasher.update([1]);
                update_identity_field(&mut hasher, value.as_bytes());
            }
            None => hasher.update([0]),
        }
    }
    hex::encode(hasher.finalize())
}

fn update_identity_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}
