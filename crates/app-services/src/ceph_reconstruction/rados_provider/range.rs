use std::sync::Arc;

use super::cache::{copy_verified_segment, VerifiedObject, VerifiedObjectCache, PAGE_BYTES};
use super::device_io::read_plan_page;
use super::{
    RadosProviderError, RbdObjectProvider, RbdObjectProviderError, RbdObjectReadOutcome,
    RbdObjectReadRequest, SourceDbRadosObjectProvider,
};

const MAX_COALESCED_PAGES: usize = 4;

impl SourceDbRadosObjectProvider {
    fn load_verified_pages(
        &mut self,
        request: &RbdObjectReadRequest,
        first_page_offset: u64,
        page_count: usize,
    ) -> Result<Vec<(u64, VerifiedObject)>, RadosProviderError> {
        let length = page_count
            .checked_mul(PAGE_BYTES)
            .ok_or_else(|| object_read_error("verified page batch length overflow"))?;
        let batch_request = RbdObjectReadRequest {
            object_no: request.object_no,
            object_identity: request.object_identity.clone(),
            object_offset: first_page_offset,
            length,
        };
        let mut expected: Option<Vec<u8>> = None;
        let mut present_count = 0usize;
        for replica_index in 0..self.replicas.len() {
            let inventory_id = self.replicas[replica_index].binding.inventory_id.clone();
            let Some((device, plan)) =
                self.resolve_replica_object(replica_index, &batch_request)?
            else {
                continue;
            };
            let bytes =
                read_plan_page(device, plan, first_page_offset, length).map_err(|error| {
                    RadosProviderError::ObjectRead {
                        inventory_id,
                        detail: error.to_string(),
                    }
                })?;
            present_count += 1;
            if let Some(reference) = &expected {
                if reference != &bytes {
                    return Err(object_read_error(
                        "RBD replicas returned conflicting object bytes",
                    ));
                }
            } else {
                expected = Some(bytes);
            }
        }
        self.validate_replica_presence(&batch_request, present_count)?;
        match expected {
            Some(bytes) => split_verified_pages(first_page_offset, bytes),
            None => missing_pages(first_page_offset, page_count),
        }
    }

    fn load_and_cache_pages(
        &mut self,
        request: &RbdObjectReadRequest,
        page_offset: u64,
        request_end: u64,
    ) -> Result<(), RbdObjectProviderError> {
        let page_count =
            coalesced_page_count(&self.verified_objects, request, page_offset, request_end);
        let pages = self
            .load_verified_pages(request, page_offset, page_count)
            .map_err(|source| RbdObjectProviderError::ReadFailed {
                object_identity: request.object_identity.clone(),
                reason: source.to_string(),
            })?;
        for (offset, page) in pages {
            self.verified_objects
                .insert(&request.object_identity, offset, page);
        }
        Ok(())
    }
}

impl RbdObjectProvider for SourceDbRadosObjectProvider {
    fn read_object_range(
        &mut self,
        request: &RbdObjectReadRequest,
        output: &mut [u8],
    ) -> Result<RbdObjectReadOutcome, RbdObjectProviderError> {
        validate_request(self, request, output)?;
        let request_end = checked_request_end(request)?;
        let mut cursor = request.object_offset;
        let mut output_offset = 0usize;
        let mut any_page_present = false;
        while cursor < request_end {
            let page_offset = aligned_page_offset(request, cursor)?;
            let segment_end = request_end.min(checked_page_end(request, page_offset)?);
            let segment_length = checked_segment_length(request, cursor, segment_end)?;
            let segment_request = RbdObjectReadRequest {
                object_no: request.object_no,
                object_identity: request.object_identity.clone(),
                object_offset: cursor,
                length: segment_length,
            };
            if !self
                .verified_objects
                .contains(&request.object_identity, page_offset)
            {
                self.load_and_cache_pages(request, page_offset, request_end)?;
            }
            let verified = self
                .verified_objects
                .get(&request.object_identity, page_offset)
                .ok_or_else(|| RbdObjectProviderError::ReadFailed {
                    object_identity: request.object_identity.clone(),
                    reason: "verified RBD page was not cached after loading".to_string(),
                })?;
            let output_slice = &mut output[output_offset..output_offset + segment_length];
            match copy_verified_segment(&segment_request, page_offset, output_slice, &verified)? {
                RbdObjectReadOutcome::Present { .. } => any_page_present = true,
                RbdObjectReadOutcome::Missing => output_slice.fill(0),
            }
            cursor = segment_end;
            output_offset += segment_length;
        }
        if any_page_present || output.is_empty() {
            Ok(RbdObjectReadOutcome::Present {
                object_identity: request.object_identity.clone(),
                bytes_read: output.len(),
            })
        } else {
            Ok(RbdObjectReadOutcome::Missing)
        }
    }
}

pub(super) fn coalesced_page_count(
    cache: &VerifiedObjectCache,
    request: &RbdObjectReadRequest,
    first_page_offset: u64,
    request_end: u64,
) -> usize {
    if request.length <= PAGE_BYTES {
        return 1;
    }
    let remaining = request_end.saturating_sub(first_page_offset);
    let touched_pages = remaining
        .saturating_add(PAGE_BYTES as u64 - 1)
        .checked_div(PAGE_BYTES as u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1)
        .clamp(1, MAX_COALESCED_PAGES);
    let mut page_count = 1usize;
    while page_count < touched_pages {
        let Some(offset) = first_page_offset.checked_add((page_count * PAGE_BYTES) as u64) else {
            break;
        };
        if cache.contains(&request.object_identity, offset) {
            break;
        }
        page_count += 1;
    }
    page_count
}

fn split_verified_pages(
    first_page_offset: u64,
    bytes: Vec<u8>,
) -> Result<Vec<(u64, VerifiedObject)>, RadosProviderError> {
    if bytes.is_empty() {
        return Err(object_read_error(
            "verified RBD page batch returned no bytes",
        ));
    }
    bytes
        .chunks(PAGE_BYTES)
        .enumerate()
        .map(|(index, page)| {
            let offset = first_page_offset
                .checked_add((index * PAGE_BYTES) as u64)
                .ok_or_else(|| object_read_error("verified page offset overflow"))?;
            Ok((offset, VerifiedObject::Present(Arc::from(page))))
        })
        .collect()
}

fn missing_pages(
    first_page_offset: u64,
    page_count: usize,
) -> Result<Vec<(u64, VerifiedObject)>, RadosProviderError> {
    (0..page_count)
        .map(|index| {
            let offset = first_page_offset
                .checked_add((index * PAGE_BYTES) as u64)
                .ok_or_else(|| object_read_error("missing page offset overflow"))?;
            Ok((offset, VerifiedObject::Missing))
        })
        .collect()
}

fn validate_request(
    provider: &SourceDbRadosObjectProvider,
    request: &RbdObjectReadRequest,
    output: &[u8],
) -> Result<(), RbdObjectProviderError> {
    if output.len() != request.length {
        return Err(read_failed(
            request,
            "provider output length does not match request",
        ));
    }
    if provider.replicas.len() != provider.expected_replica_count {
        return Err(RbdObjectProviderError::Unavailable {
            object_identity: request.object_identity.clone(),
            reason: "RBD replica coverage is no longer closed".to_string(),
        });
    }
    Ok(())
}

fn checked_request_end(request: &RbdObjectReadRequest) -> Result<u64, RbdObjectProviderError> {
    request
        .object_offset
        .checked_add(request.length as u64)
        .ok_or_else(|| read_failed(request, "RBD request range overflow"))
}

fn aligned_page_offset(
    request: &RbdObjectReadRequest,
    cursor: u64,
) -> Result<u64, RbdObjectProviderError> {
    (cursor / PAGE_BYTES as u64)
        .checked_mul(PAGE_BYTES as u64)
        .ok_or_else(|| read_failed(request, "RBD page offset overflow"))
}

fn checked_page_end(
    request: &RbdObjectReadRequest,
    page_offset: u64,
) -> Result<u64, RbdObjectProviderError> {
    page_offset
        .checked_add(PAGE_BYTES as u64)
        .ok_or_else(|| read_failed(request, "RBD page end overflow"))
}

fn checked_segment_length(
    request: &RbdObjectReadRequest,
    cursor: u64,
    segment_end: u64,
) -> Result<usize, RbdObjectProviderError> {
    usize::try_from(segment_end - cursor)
        .map_err(|_| read_failed(request, "RBD page segment does not fit in memory"))
}

fn read_failed(request: &RbdObjectReadRequest, reason: &str) -> RbdObjectProviderError {
    RbdObjectProviderError::ReadFailed {
        object_identity: request.object_identity.clone(),
        reason: reason.to_string(),
    }
}

fn object_read_error(detail: &str) -> RadosProviderError {
    RadosProviderError::ObjectRead {
        inventory_id: "replica-set".to_string(),
        detail: detail.to_string(),
    }
}
