use super::run_emulation_blocking;

#[tokio::test]
async fn emulation_worker_returns_the_operation_result() {
    let value = run_emulation_blocking("test", || Ok::<_, transport::CommandError>(42))
        .await
        .unwrap();
    assert_eq!(value, 42);
}

#[tokio::test]
async fn emulation_worker_converts_a_panic_to_a_typed_error() {
    let error = run_emulation_blocking::<(), _>("test", || panic!("intentional test panic"))
        .await
        .unwrap_err();
    assert_eq!(error.code, "INTERNAL");
    assert_eq!(error.category, "internal");
}
