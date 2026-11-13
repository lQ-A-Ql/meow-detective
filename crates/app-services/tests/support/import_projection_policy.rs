pub(super) fn requested() -> app_services::timeline_service::TimelineProjectionPolicy {
    let mut policy = app_services::timeline_service::TimelineProjectionPolicy::default();
    match std::env::var("FORENSICS_TIMELINE_WORKSPACE_KIB") {
        Ok(value) => policy.workspace_kib = value.parse().expect("workspace KiB"),
        Err(std::env::VarError::NotPresent) => {}
        Err(error) => panic!("invalid workspace KiB environment: {error}"),
    }
    policy.validate().expect("bounded projection policy");
    policy
}

pub(super) fn describe() -> serde_json::Value {
    let policy = requested();
    serde_json::json!({
        "workspaceKiB": policy.workspace_kib,
        "reservationBytes": policy.reservation_bytes().expect("validated workspace reservation"),
        "reservationKind": "estimatedWorkspace"
    })
}
