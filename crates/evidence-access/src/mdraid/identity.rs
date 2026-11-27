use domain::structure::SourceRevisionRef;
use sha2::{Digest, Sha256};

pub fn registered_source_id(
    array_uuid: &[u8; 16],
    fingerprint: &str,
    inputs: &[SourceRevisionRef],
) -> String {
    let mut ordered = inputs.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        (&left.case_id, &left.source_id, left.revision).cmp(&(
            &right.case_id,
            &right.source_id,
            right.revision,
        ))
    });
    let mut digest = Sha256::new();
    digest.update(b"mdraid-registered-inputs-v1");
    digest.update((ordered.len() as u64).to_le_bytes());
    for input in ordered {
        for value in [&input.case_id, &input.source_id] {
            digest.update((value.len() as u64).to_le_bytes());
            digest.update(value.as_bytes());
        }
        digest.update(input.revision.to_le_bytes());
    }
    digest.update((fingerprint.len() as u64).to_le_bytes());
    digest.update(fingerprint.as_bytes());
    format!(
        "mdraid-v1-{}-{}",
        hex::encode(array_uuid),
        hex::encode(digest.finalize())
    )
}
