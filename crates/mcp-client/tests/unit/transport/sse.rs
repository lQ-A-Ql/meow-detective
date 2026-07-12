use super::*;

#[test]
fn test_sse_transport_new() {
    let transport = SseTransport::new("http://localhost:3001").unwrap();
    assert_eq!(transport.url, "http://localhost:3001/");
    assert!(!transport.is_connected());
}

#[test]
fn test_request_id_increment() {
    let transport = SseTransport::new("http://localhost:3001").unwrap();
    let id1 = transport.request_id.fetch_add(1, Ordering::SeqCst);
    let id2 = transport.request_id.fetch_add(1, Ordering::SeqCst);
    assert_eq!(id2, id1 + 1);
}

#[test]
fn test_sse_transport_rejects_invalid_url() {
    let Err(err) = SseTransport::new("file:///tmp/mcp.sock") else {
        panic!("expected invalid SSE URL to fail");
    };
    assert!(err.to_string().contains("Unsupported MCP SSE URL scheme"));
}
