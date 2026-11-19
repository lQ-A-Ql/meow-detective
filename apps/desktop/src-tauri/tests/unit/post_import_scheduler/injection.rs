use super::*;

#[test]
fn unset_env_resolves_to_zero() {
    assert_eq!(parse_post_import_delay_ms(None), 0);
}

#[test]
fn empty_string_resolves_to_zero() {
    assert_eq!(parse_post_import_delay_ms(Some("")), 0);
}

#[test]
fn zero_resolves_to_zero() {
    assert_eq!(parse_post_import_delay_ms(Some("0")), 0);
}

#[test]
fn invalid_text_resolves_to_zero() {
    assert_eq!(parse_post_import_delay_ms(Some("abc")), 0);
}

#[test]
fn negative_value_resolves_to_zero() {
    assert_eq!(parse_post_import_delay_ms(Some("-5")), 0);
}

#[test]
fn positive_value_is_parsed_with_trim() {
    assert_eq!(parse_post_import_delay_ms(Some(" 1500 ")), 1500);
}
