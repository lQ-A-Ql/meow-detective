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
#[path = "../tests/unit/error_ext.rs"]
mod tests;
