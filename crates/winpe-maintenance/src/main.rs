use std::process::ExitCode;

use winpe_maintenance::{
    ensure_winpe_runtime, find_single_windows_installation, inspect_osdata, remove_osdata,
    windows_drive_roots, OsdataState,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("maintenance_error={error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    ensure_winpe_runtime()?;
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let windows_root = find_single_windows_installation(windows_drive_roots())?;
    match arguments.as_slice() {
        [operation] if operation == "probe-osdata" => {
            print_result("probe-osdata", inspect_osdata(&windows_root)?);
        }
        [operation, confirmation] if operation == "remove-osdata" && confirmation == "--apply" => {
            print_result("remove-osdata", remove_osdata(&windows_root)?);
        }
        _ => {
            return Err(
                "usage: meow-winpe-maintenance <probe-osdata|remove-osdata --apply>".into(),
            );
        }
    }
    Ok(())
}

fn print_result(operation: &str, state: OsdataState) {
    println!("operation={operation}");
    println!("result={state:?}");
}
