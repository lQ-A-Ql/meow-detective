use timeline::TimelineProjectionPolicy;

#[test]
fn workspace_reservation_preserves_kib_units_and_rejects_invalid_values() {
    for kib in [128 * 1024, 1_000_000, 1024 * 1024] {
        let policy = TimelineProjectionPolicy { workspace_kib: kib };
        assert_eq!(policy.reservation_bytes().unwrap(), u64::from(kib) * 1024);
    }
    for kib in [0, 128 * 1024 - 1, 1024 * 1024 + 1] {
        assert!(TimelineProjectionPolicy { workspace_kib: kib }
            .reservation_bytes()
            .is_err());
    }
}
