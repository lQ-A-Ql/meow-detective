//! Bash history parser.
//!
//! Parses `.bash_history` files with optional HISTTIMEFORMAT support.
//! When HISTTIMEFORMAT is set in the shell environment, bash prepends
//! `#<epoch_seconds>` lines before each command.
//!
//! Format:
//! ```text
//! #1234567890
//! ls -la
//! #1234567900
//! cat /etc/passwd
//! git status
//! ```

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// A single command from a bash history file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BashCommand {
    /// The shell command text
    pub command: String,
    /// Timestamp if HISTTIMEFORMAT is enabled
    pub timestamp: Option<DateTime<Utc>>,
    /// 1-indexed line number in the history file
    pub line_number: u64,
}

/// Parse a `.bash_history` file.
///
/// Supports the HISTTIMEFORMAT convention where timestamp lines starting with `#`
/// followed by an epoch timestamp precede each command.
///
/// Lines that are blank or only contain `#` followed by non-numeric content are skipped.
pub fn parse_bash_history(content: &str) -> Result<Vec<BashCommand>, crate::LinuxArtifactError> {
    let mut commands: Vec<BashCommand> = Vec::new();
    let mut pending_timestamp: Option<i64> = None;

    for (line_number, line) in (1u64..).zip(content.lines()) {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Check for HISTTIMEFORMAT timestamp line: #<epoch_seconds>
        if let Some(rest) = trimmed.strip_prefix('#') {
            if let Ok(epoch) = rest.parse::<i64>() {
                // Sanity check: epoch should be between 1990-01-01 and 2100-01-01
                if epoch > 631_152_000 && epoch < 4_102_444_800 {
                    pending_timestamp = Some(epoch);
                    continue;
                }
            }
            // Lines starting with # that aren't a valid epoch timestamp are comments; skip
            continue;
        }

        let timestamp = pending_timestamp
            .take()
            .and_then(|epoch| Utc.timestamp_opt(epoch, 0).single());

        commands.push(BashCommand {
            command: trimmed.to_string(),
            timestamp,
            line_number,
        });
    }

    Ok(commands)
}

#[cfg(test)]
#[path = "../tests/unit/bash.rs"]
mod tests;
