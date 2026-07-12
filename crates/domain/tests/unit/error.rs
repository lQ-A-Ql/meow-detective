use super::*;

#[test]
fn error_display() {
    let err = ForensicsError::NotFound("file.txt".to_string());
    assert_eq!(err.to_string(), "Not found: file.txt");
}

#[test]
fn error_from_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err = ForensicsError::from(io_err);
    assert!(err.to_string().contains("IO error"));
}

#[test]
fn error_from_string() {
    let err = ForensicsError::from("test error");
    assert!(err.to_string().contains("Internal error"));
}

#[test]
fn error_from_boxed_error() {
    let boxed_err: Box<dyn std::error::Error> = Box::new(std::io::Error::other("test error"));
    let err = ForensicsError::from(boxed_err);
    assert!(err.to_string().contains("Internal error"));
}

#[test]
fn error_cancelled() {
    let err = ForensicsError::Cancelled;
    assert_eq!(err.to_string(), "Operation cancelled");
}

#[test]
fn error_not_supported() {
    let err = ForensicsError::NotSupported("feature X".to_string());
    assert_eq!(err.to_string(), "Not supported: feature X");
}
