use super::*;

#[test]
fn page_request_clamps_unbounded_limit() {
    let mut request = PageRequest {
        offset: 0,
        limit: u32::MAX,
    };

    request.clamp();

    assert_eq!(request.limit, PageRequest::MAX_LIMIT);
}

#[test]
fn page_request_replaces_zero_with_default() {
    let mut request = PageRequest {
        offset: 0,
        limit: 0,
    };

    request.clamp();

    assert_eq!(request.limit, PageRequest::DEFAULT_LIMIT);
}
