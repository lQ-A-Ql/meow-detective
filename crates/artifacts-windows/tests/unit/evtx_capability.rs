use super::*;

#[test]
fn capability_documents_structured_event_coverage() {
    let capability = evtx_capability();
    assert_eq!(capability.parser_id, "evtx.boot_shutdown");
    assert_eq!(capability.supported_event_ids, SUPPORTED_EVENT_IDS);
    assert!(capability.max_input_bytes > 0);
}
