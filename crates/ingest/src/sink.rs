use domain::FileEntry;

/// Sink for receiving ingestion results.
///
/// Implementations persist file entries and events to their respective stores.
pub trait IngestSink: Send {
    /// Receive a batch of file entries.
    fn push_files(&mut self, entries: &[FileEntry]) -> Result<(), String>;

    /// Report progress (files processed so far, total estimate).
    fn report_progress(&self, processed: u64, total: u64);

    /// Report a warning message.
    fn report_warning(&mut self, message: &str);

    /// Report an error message.
    fn report_error(&mut self, message: &str);
}
