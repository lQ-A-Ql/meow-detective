use std::io::Write;

use tracing::error;

use super::{crash_report_dir, CrashReport};

pub(crate) fn write_crash_report(report: &CrashReport) {
    let dir = crash_report_dir();
    if let Err(error) = std::fs::create_dir_all(&dir) {
        error!(target = "crash_handler", dir = %dir.display(), %error, "failed to create crash report directory");
        return;
    }

    let filepath = dir.join(format!("crash-{}.json", report.timestamp));
    let json = match serde_json::to_string_pretty(report) {
        Ok(json) => json,
        Err(error) => {
            error!(target = "crash_handler", %error, "failed to serialize crash report");
            return;
        }
    };

    match std::fs::File::create(&filepath) {
        Ok(mut file) => {
            if let Err(error) = file.write_all(json.as_bytes()) {
                error!(target = "crash_handler", path = %filepath.display(), %error, "failed to write crash report file");
            } else {
                error!(target = "crash_handler", path = %filepath.display(), "crash report saved");
            }
        }
        Err(error) => {
            error!(target = "crash_handler", path = %filepath.display(), %error, "failed to write crash report file");
        }
    }
}
