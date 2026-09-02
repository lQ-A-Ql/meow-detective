use super::*;

#[test]
fn detects_always_restart_and_remote_session_termination() {
    let unit = r#"
        [Service]
        ExecStart=/usr/bin/python /usr/local/bin/network-check.py
        Restart=always
    "#;
    let script = r#"
        process = subprocess.Popen(['who'], stdout=subprocess.PIPE)
        if 'pts' in tty:
            os.system("pkill -9 -t {}".format(tty))
    "#;
    assert_eq!(
        analyze_unit(unit, |path| {
            assert_eq!(path, "/usr/local/bin/network-check.py");
            Some(script.to_string())
        }),
        UnitRiskAnalysis {
            always_restarts: true,
            terminates_remote_sessions: true,
        }
    );
}

#[test]
fn ignores_comments_and_standard_restart_policy() {
    let unit = r#"
        [Service]
        ; Restart=always
        ExecStart=/usr/sbin/sshd -D
        Restart=on-failure
    "#;
    assert_eq!(
        analyze_unit(unit, |_| panic!("binary services are not read as scripts")),
        UnitRiskAnalysis::default()
    );
}

#[test]
fn remote_session_detection_requires_all_signals() {
    assert!(!script_terminates_remote_sessions("pkill -9 -t pts/0"));
    assert!(!script_terminates_remote_sessions(
        "subprocess.Popen(['who']); print('pts')"
    ));
}
