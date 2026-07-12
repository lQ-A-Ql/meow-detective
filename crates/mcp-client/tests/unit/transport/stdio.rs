use super::*;

#[test]
fn test_stdio_transport_new() {
    let transport =
        StdioTransport::new("python", &["-m".to_string(), "server".to_string()]).unwrap();
    assert_eq!(transport.command, "python");
    assert_eq!(transport.args, vec!["-m", "server"]);
    assert!(!transport.is_connected());
}

#[test]
fn test_request_id_increment() {
    let transport = StdioTransport::new("echo", &[]).unwrap();
    let id1 = transport.request_id.fetch_add(1, Ordering::SeqCst);
    let id2 = transport.request_id.fetch_add(1, Ordering::SeqCst);
    assert_eq!(id2, id1 + 1);
}

#[test]
fn test_stdio_transport_rejects_empty_command() {
    let Err(err) = StdioTransport::new("  ", &[]) else {
        panic!("expected empty stdio command to fail");
    };
    assert!(err.to_string().contains("stdio command is required"));
}
