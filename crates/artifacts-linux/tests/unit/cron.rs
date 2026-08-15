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
    let jobs = parse_crontab_with_source_and_kind(
        "0 5 * * * /usr/bin/mysql_backup",
        "/etc/cron.d/mysql",
        CrontabKind::System,
    )
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

#[test]
fn user_crontab_kind_treats_bare_word_as_command() {
    let input = "0 0 * * * echo hello";
    let jobs = parse_crontab_with_source_and_kind(input, "<unknown>", CrontabKind::User)
        .expect("should parse");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].user, None);
    assert_eq!(jobs[0].command, "echo hello");
}

#[test]
fn system_crontab_kind_reads_user_field() {
    let input = "0 0 * * * echo hello";
    let jobs = parse_crontab_with_source_and_kind(input, "<unknown>", CrontabKind::System)
        .expect("should parse");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].user.as_deref(), Some("echo"));
    assert_eq!(jobs[0].command, "hello");
}

#[test]
fn user_crontab_kind_keyword_schedule_has_no_user() {
    let input = "@daily echo housekeeping";
    let jobs = parse_crontab_with_source_and_kind(input, "<unknown>", CrontabKind::User)
        .expect("should parse");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].user, None);
    assert_eq!(jobs[0].command, "echo housekeeping");
}

#[test]
fn system_crontab_kind_keyword_schedule_with_user() {
    let input = "@daily root /usr/bin/housekeeping";
    let jobs = parse_crontab_with_source_and_kind(input, "<unknown>", CrontabKind::System)
        .expect("should parse");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].user.as_deref(), Some("root"));
    assert_eq!(jobs[0].command, "/usr/bin/housekeeping");
}

#[test]
fn midnight_keyword_is_a_schedule() {
    let input = "@midnight /usr/bin/nightly";
    let jobs = parse_crontab_with_source_and_kind(input, "<unknown>", CrontabKind::User)
        .expect("should parse");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].schedule, "@midnight");
    assert_eq!(jobs[0].command, "/usr/bin/nightly");
}

#[test]
fn legacy_parse_crontab_defaults_to_system_semantics() {
    let jobs = parse_crontab("0 0 * * * root /usr/bin/task").expect("should parse");
    assert_eq!(jobs[0].user.as_deref(), Some("root"));
    assert_eq!(jobs[0].command, "/usr/bin/task");
}

#[test]
fn shell_script_lines_are_not_cron_jobs() {
    // /etc/cron.daily/* style scripts must never be split into fake
    // schedules and commands.
    let input = "\
#!/bin/sh
if [ \"$CRON\" = \"no\" ]; then
    exit 0
fi
ionice -c3 -p $$ >/dev/null 2>&1
/usr/sbin/logrotate /etc/logrotate.conf
EXITVALUE=$?
0 3 * * * /usr/bin/weekly-task";
    let jobs = parse_crontab_with_source_and_kind(input, "<unknown>", CrontabKind::System)
        .expect("should parse");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].schedule, "0 3 * * *");
    assert_eq!(jobs[0].command, "/usr/bin/weekly-task");
}

#[test]
fn schedule_fields_accept_list_range_step() {
    let input = "5,15 1-3 */2 * 1,5 /usr/bin/combo";
    let jobs = parse_crontab_with_source_and_kind(input, "<unknown>", CrontabKind::User)
        .expect("should parse");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].schedule, "5,15 1-3 */2 * 1,5");
    assert_eq!(jobs[0].command, "/usr/bin/combo");
}

#[test]
fn unknown_at_keyword_is_rejected() {
    let jobs = parse_crontab_with_source_and_kind(
        "@fortnightly /usr/bin/task",
        "<unknown>",
        CrontabKind::User,
    )
    .expect("should parse");
    assert!(jobs.is_empty());
}
