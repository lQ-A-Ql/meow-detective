/// Line-level parse accounting shared by the text log parsers: how many
/// non-blank input lines were seen and how many yielded a structured entry.
/// The counters let callers warn on logs the parser does not understand
/// instead of silently dropping lines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LogLineStats {
    /// Non-blank input lines seen by the parser.
    pub total_lines: u64,
    /// Lines that yielded a structured entry.
    pub parsed_lines: u64,
}

impl LogLineStats {
    /// Lines seen but not parsed into entries.
    pub fn unparsed_lines(self) -> u64 {
        self.total_lines.saturating_sub(self.parsed_lines)
    }
}

/// Parse accounting for web access logs: line counters plus the number of
/// parsed lines that carried a virtual-host (`%v`/`$host`) prefix.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WebAccessLogStats {
    /// Non-blank input lines seen by the parser.
    pub total_lines: u64,
    /// Lines that yielded a structured entry.
    pub parsed_lines: u64,
    /// Parsed lines whose client address was recovered from the second field
    /// because a virtual-host name led the line.
    pub vhost_prefixed_lines: u64,
}

impl WebAccessLogStats {
    /// Lines seen but not parsed into entries.
    pub fn unparsed_lines(self) -> u64 {
        self.total_lines.saturating_sub(self.parsed_lines)
    }
}
