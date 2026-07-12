use super::*;

#[test]
fn parse_sudo_command_lines() {
    let input = "\
Jan 15 10:30:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/apt update
Jan 15 10:30:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)
Jan 15 10:32:00 ubuntu sudo:     bob : TTY=pts/1 ; PWD=/home/bob ; USER=root ; COMMAND=/usr/bin/systemctl restart nginx
Jan 15 10:32:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by bob(uid=0)
Jan 15 10:35:00 ubuntu sudo: pam_unix(sudo:session): session closed for user root";
    let events = parse_auth_log_sudo(input).expect("should parse auth log");
    let cmds: Vec<&SudoEvent> = events
        .iter()
        .filter(|e| !e.command.contains("authentication failure"))
        .collect();
    assert_eq!(cmds.len(), 2);
    assert_eq!(cmds[0].user, "alice");
    assert_eq!(cmds[0].command, "/usr/bin/apt update");
    assert_eq!(cmds[0].working_directory.as_deref(), Some("/home/alice"));
    assert_eq!(cmds[0].target_user.as_deref(), Some("root"));
    assert_eq!(cmds[0].terminal.as_deref(), Some("pts/0"));
    assert!(cmds[0].success);
    assert_eq!(cmds[1].user, "bob");
    assert_eq!(cmds[1].command, "/usr/bin/systemctl restart nginx");
    assert!(cmds[1].success);
}

#[test]
fn command_before_session_open_is_successful() {
    let input = "\
Jan 15 10:30:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/id
Jan 15 10:30:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)";
    let events = parse_auth_log_sudo(input).expect("should parse auth log");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command, "/usr/bin/id");
    assert!(events[0].success);
}

#[test]
fn parse_sudo_auth_failure() {
    let input = "\
Jan 15 10:30:00 ubuntu sudo: pam_unix(sudo:auth): authentication failure; logname=alice uid=1000 euid=0 tty=/dev/pts/0 ruser=alice rhost=  user=alice
Jan 15 10:30:01 ubuntu sudo:   alice : 3 incorrect password attempts ; TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/su";
    let events = parse_auth_log_sudo(input).expect("should parse");
    let failure = events
        .iter()
        .find(|e| e.command == "[authentication failure]")
        .expect("authentication failure event");
    assert_eq!(failure.user, "alice");
    assert!(!failure.success);
}

#[test]
fn parse_empty_input() {
    assert!(parse_auth_log_sudo("").expect("should parse").is_empty());
}

#[test]
fn skip_non_sudo_lines() {
    let input = "\
Jan 15 10:30:00 ubuntu sshd[1234]: Accepted publickey for alice from 192.168.1.100 port 22
Jan 15 10:30:01 ubuntu CRON[5678]: (root) CMD (test -x /usr/sbin/anacron || ...)
Jan 15 10:30:02 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/whoami";
    let events = parse_auth_log_sudo(input).expect("should parse");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].user, "alice");
}

#[test]
fn rhel_secure_log_format() {
    let input = "\
Jan 15 10:30:00 centos sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/yum update
Jan 15 10:30:05 centos sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)";
    let events = parse_auth_log_sudo(input).expect("should parse RHEL secure log");
    let cmds: Vec<&SudoEvent> = events
        .iter()
        .filter(|e| !e.command.contains("authentication failure"))
        .collect();
    assert!(!cmds.is_empty());
    assert_eq!(cmds[0].command, "/usr/bin/yum update");
}
