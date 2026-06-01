pub fn crate_name() -> &'static str {
    "image-raw"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "image-raw");
    }
}
