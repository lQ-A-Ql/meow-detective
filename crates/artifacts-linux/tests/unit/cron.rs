use super::*;

#[test]
fn parse_user_crontab() {
    let input = "\
# Edit this file to introduce tasks to be run by cron.
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin

# m h  dom mon dow   command
30 2 * * * /usr/bin/backup.sh
0 */6 * * * /usr/local/bin/cleanup.sh
@daily /usr/bin/logrotate";
    let jobs = parse_crontab(input).expect("should parse crontab");
    assert_eq!(jobs.len(), 3);
    assert_eq!(jobs[0].schedule, "30 2 * * *");
    assert_eq!(jobs[0].user, None);
    assert_eq!(jobs[0].command, "/usr/bin/backup.sh");
    assert_eq!(jobs[1].schedule, "0 */6 * * *");
    assert_eq!(jobs[1].command, "/usr/local/bin/cleanup.sh");
    assert_eq!(jobs[2].schedule, "@daily");
    assert_eq!(jobs[2].command, "/usr/bin/logrotate");
}

#[test]
fn parse_system_crontab_with_user_field() {
    let input = "\
SHELL=/bin/sh
PATH=/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin

17 * * * * root cd / && run-parts --report /etc/cron.hourly
25 6 * * * root test -x /usr/sbin/anacron || ( cd / && run-parts --report /etc/cron.daily )
@reboot root /usr/local/bin/startup.sh";
    let jobs = parse_crontab(input).expect("should parse system crontab");
    assert_eq!(jobs.len(), 3);
    assert_eq!(jobs[0].schedule, "17 * * * *");
    assert_eq!(jobs[0].user.as_deref(), Some("root"));
    assert!(jobs[0].command.contains("run-parts"));
    assert_eq!(jobs[2].schedule, "@reboot");
    assert_eq!(jobs[2].user.as_deref(), Some("root"));
    assert_eq!(jobs[2].command, "/usr/local/bin/startup.sh");
}

#[test]
fn parse_with_source_file() {
    let jobs = parse_crontab_with_source("0 5 * * * /usr/bin/mysql_backup", "/etc/cron.d/mysql")
        .expect("should parse");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].source_file, "/etc/cron.d/mysql");
    assert_eq!(jobs[0].command, "/usr/bin/mysql_backup");
}

#[test]
fn parse_keyword_schedules() {
    let input = "\
@yearly /usr/bin/annual-report
@monthly /usr/bin/monthly-cleanup
@weekly /usr/bin/weekly-backup
@daily /usr/bin/daily-sync
@hourly /usr/bin/hourly-check
@reboot /usr/bin/startup";
    let jobs = parse_crontab(input).expect("should parse");
    assert_eq!(jobs.len(), 6);
    assert_eq!(jobs[0].schedule, "@yearly");
    assert_eq!(jobs[5].schedule, "@reboot");
}

#[test]
fn skip_comments_and_blanks() {
    let input =
        "\n# This is a comment\n# Another comment\n0 0 * * * /usr/bin/midnight-task\n\n# End\n";
    assert_eq!(parse_crontab(input).expect("should parse").len(), 1);
}

#[test]
fn empty_input() {
    assert!(parse_crontab("").expect("should parse").is_empty());
}

#[test]
fn skip_env_assignments() {
    let input = "SHELL=/bin/bash\nMAILTO=admin@example.com\nHOME=/root\n0 0 * * * /usr/bin/task";
    let jobs = parse_crontab(input).expect("should parse");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].command, "/usr/bin/task");
}
