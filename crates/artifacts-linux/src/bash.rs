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
mod tests {
    use super::*;

    #[test]
    fn parse_with_timestamps() {
        let input = "\
#1234567890
ls -la /home
#1234567900
cat /etc/hostname
#1234567910
echo hello world";

        let cmds = parse_bash_history(input).expect("should parse");
        assert_eq!(cmds.len(), 3);

        assert_eq!(cmds[0].command, "ls -la /home");
        assert!(cmds[0].timestamp.is_some());
        assert_eq!(cmds[0].timestamp.unwrap().timestamp(), 1234567890);

        assert_eq!(cmds[1].command, "cat /etc/hostname");
        assert_eq!(cmds[2].command, "echo hello world");

        // Line numbers are 1-indexed
        assert_eq!(cmds[0].line_number, 2); // line 2 (line 1 was the timestamp)
        assert_eq!(cmds[1].line_number, 4);
        assert_eq!(cmds[2].line_number, 6);
    }

    #[test]
    fn parse_without_timestamps() {
        let input = "\
ls
pwd
whoami";

        let cmds = parse_bash_history(input).expect("should parse");
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[0].command, "ls");
        assert!(cmds[0].timestamp.is_none());
        assert_eq!(cmds[1].command, "pwd");
        assert_eq!(cmds[2].command, "whoami");
    }

    #[test]
    fn skip_comments() {
        let input = "\
# This is a comment
#1234567890
actual command
# another comment
#1234567900
second command";

        let cmds = parse_bash_history(input).expect("should parse");
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].command, "actual command");
        assert_eq!(cmds[1].command, "second command");
    }

    #[test]
    fn empty_input() {
        let cmds = parse_bash_history("").expect("should parse");
        assert!(cmds.is_empty());
    }

    #[test]
    fn trailing_timestamp_no_command() {
        let input = "\
ls
#1234567890";

        let cmds = parse_bash_history(input).expect("should parse");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].command, "ls");
        assert!(cmds[0].timestamp.is_none());
    }

    #[test]
    fn command_after_command_reuses_no_timestamp() {
        // Two consecutive commands without a timestamp line between them
        let input = "\
#1234567890
cmd1
cmd2";

        let cmds = parse_bash_history(input).expect("should parse");
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].command, "cmd1");
        assert!(cmds[0].timestamp.is_some());
        assert_eq!(cmds[1].command, "cmd2");
        // cmd2 gets no timestamp because pending_timestamp was consumed by cmd1
        assert!(cmds[1].timestamp.is_none());
    }
}
