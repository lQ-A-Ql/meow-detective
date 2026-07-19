use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use super::{CephFsObjectReadError, ResolvedReplica};
use crate::ceph_reconstruction::RadosObjectReader;

pub(super) fn read_and_verify(
    locator: &str,
    offset: u64,
    length: usize,
    replicas: &[ResolvedReplica],
) -> Result<Vec<u8>, CephFsObjectReadError> {
    let mut expected = None;
    for replica in replicas {
        let mut reader = RadosObjectReader::from_layout(
            Box::new(replica.device.clone()),
            Arc::clone(&replica.layout),
        );
        reader
            .seek(SeekFrom::Start(offset))
            .and_then(|_| {
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).map(|_| bytes)
            })
            .map_err(|_| CephFsObjectReadError::ObjectRead {
                inventory_id: replica.provenance.inventory_id.clone(),
            })
            .and_then(|bytes| {
                if expected.as_ref().is_some_and(|expected| expected != &bytes) {
                    Err(CephFsObjectReadError::ByteConflict {
                        locator: locator.to_string(),
                    })
                } else {
                    expected = Some(bytes);
                    Ok(())
                }
            })?;
    }
    Ok(expected.unwrap_or_default())
}
