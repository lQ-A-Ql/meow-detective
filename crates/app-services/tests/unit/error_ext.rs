use super::*;

#[test]
fn test_to_string_err_ok() {
    let result: Result<i32, i32> = Ok(42);
    assert_eq!(result.to_string_err(), Ok(42));
}

#[test]
fn test_to_string_err_error() {
    let result: Result<i32, &str> = Err("error message");
    assert_eq!(result.to_string_err(), Err("error message".to_string()));
}

#[test]
fn test_context_ok() {
    let result: Result<i32, i32> = Ok(42);
    assert_eq!(result.context("context"), Ok(42));
}

#[test]
fn test_context_error() {
    let result: Result<i32, &str> = Err("error");
    assert_eq!(result.context("context"), Err("context: error".to_string()));
}

#[test]
fn test_to_string_err_with_io_error() {
    let result: Result<i32, std::io::Error> = Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file not found",
    ));
    let err = result.to_string_err().unwrap_err();
    assert!(err.contains("file not found"));
}
