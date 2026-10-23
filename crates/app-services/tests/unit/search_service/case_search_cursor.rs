use super::validate_window;

#[test]
fn cursor_window_can_exceed_the_legacy_offset_limit() {
    let total = 1_421_594;

    validate_window(None, total, total, 100_000)
        .expect("cursor pagination must remain valid beyond the legacy offset window");
}
