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
    assert_eq!(cmds[0].line_number, 2);
    assert_eq!(cmds[1].line_number, 4);
    assert_eq!(cmds[2].line_number, 6);
}

#[test]
fn parse_without_timestamps() {
    let input = "ls\npwd\nwhoami";
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
    assert!(parse_bash_history("").expect("should parse").is_empty());
}

#[test]
fn trailing_timestamp_no_command() {
    let cmds = parse_bash_history("ls\n#1234567890").expect("should parse");
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].command, "ls");
    assert!(cmds[0].timestamp.is_none());
}

#[test]
fn command_after_command_reuses_no_timestamp() {
    let cmds = parse_bash_history("#1234567890\ncmd1\ncmd2").expect("should parse");
    assert_eq!(cmds.len(), 2);
    assert_eq!(cmds[0].command, "cmd1");
    assert!(cmds[0].timestamp.is_some());
    assert_eq!(cmds[1].command, "cmd2");
    assert!(cmds[1].timestamp.is_none());
}
