//! Bounded compatibility checks for enabled, locally defined systemd units.

use evidence_core::FileSystemReader;

const LOCAL_UNIT_DIRECTORY: &str = "etc/systemd/system";
const MULTI_USER_WANTS: &str = "etc/systemd/system/multi-user.target.wants";
const MAX_ENABLED_UNITS: usize = 128;
const MAX_UNIT_BYTES: usize = 64 * 1024;
const MAX_SCRIPT_BYTES: usize = 256 * 1024;

pub(super) fn annotate_service_risks(fs: &dyn FileSystemReader, risks: &mut Vec<String>) {
    let Ok(enabled) = fs.list_children(MULTI_USER_WANTS) else {
        return;
    };
    for entry in enabled
        .into_iter()
        .filter(|entry| !entry.is_dir && entry.name.ends_with(".service"))
        .take(MAX_ENABLED_UNITS)
    {
        let unit_path = format!("{LOCAL_UNIT_DIRECTORY}/{}", entry.name);
        let Some(unit) = read_utf8_bounded(fs, &unit_path, MAX_UNIT_BYTES) else {
            continue;
        };
        let analysis = analyze_unit(&unit, |path| read_utf8_bounded(fs, path, MAX_SCRIPT_BYTES));
        if analysis.always_restarts {
            add_risk(risks, "custom-service-always-restart");
        }
        if analysis.terminates_remote_sessions {
            add_risk(risks, "remote-session-guard");
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct UnitRiskAnalysis {
    always_restarts: bool,
    terminates_remote_sessions: bool,
}

fn analyze_unit(unit: &str, read_script: impl Fn(&str) -> Option<String>) -> UnitRiskAnalysis {
    let always_restarts =
        unit_setting(unit, "Restart").is_some_and(|value| value.eq_ignore_ascii_case("always"));
    let script = unit_setting(unit, "ExecStart")
        .and_then(exec_start_script)
        .and_then(read_script);
    UnitRiskAnalysis {
        always_restarts,
        terminates_remote_sessions: script
            .as_deref()
            .is_some_and(script_terminates_remote_sessions),
    }
}

fn unit_setting<'a>(unit: &'a str, key: &str) -> Option<&'a str> {
    unit.lines().find_map(|line| {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            return None;
        }
        let (candidate, value) = line.split_once('=')?;
        candidate
            .trim()
            .eq_ignore_ascii_case(key)
            .then_some(value.trim())
    })
}

fn exec_start_script(command: &str) -> Option<&str> {
    command
        .split_ascii_whitespace()
        .map(|token| token.trim_matches(|character| matches!(character, '\'' | '"')))
        .find(|token| token.starts_with('/') && token.ends_with(".py"))
}

fn script_terminates_remote_sessions(script: &str) -> bool {
    let folded = script.to_ascii_lowercase();
    let enumerates_logins = folded.contains("subprocess.popen(['who']")
        || folded.contains("subprocess.popen([\"who\"]")
        || folded.contains("os.system('who")
        || folded.contains("os.system(\"who");
    let targets_remote_ttys = folded.contains("'pts'") || folded.contains("\"pts\"");
    let terminates_tty = folded.contains("pkill -9 -t") || folded.contains("pkill -t");
    enumerates_logins && targets_remote_ttys && terminates_tty
}

fn read_utf8_bounded(fs: &dyn FileSystemReader, path: &str, maximum: usize) -> Option<String> {
    let bytes = fs
        .read_file_range(path, 0, maximum.saturating_add(1))
        .ok()?;
    if bytes.len() > maximum {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn add_risk(risks: &mut Vec<String>, risk: &str) {
    if !risks.iter().any(|current| current == risk) {
        risks.push(risk.to_string());
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/emulation_linux_services.rs"]
mod tests;
