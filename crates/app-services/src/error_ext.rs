//! Error extension traits for simplified error handling.

/// Extension trait for converting errors to String.
pub trait ResultExt<T> {
    /// Convert error to String using Display.
    fn to_string_err(self) -> Result<T, String>;
}

impl<T, E: std::fmt::Display> ResultExt<T> for Result<T, E> {
    fn to_string_err(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}

/// Extension trait for converting errors with context.
pub trait ResultContext<T> {
    /// Add context to an error.
    fn context(self, msg: &str) -> Result<T, String>;
}

impl<T, E: std::fmt::Display> ResultContext<T> for Result<T, E> {
    fn context(self, msg: &str) -> Result<T, String> {
        self.map_err(|e| format!("{}: {}", msg, e))
    }
}

#[cfg(test)]
mod tests {
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
        let result: Result<i32, std::io::Error> =
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"));
        let err = result.to_string_err().unwrap_err();
        assert!(err.contains("file not found"));
    }
}
