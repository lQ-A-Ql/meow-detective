use std::process::ExitCode;

use winpe_maintenance::{
    apply_bypass, ensure_winpe_runtime, find_single_windows_installation, inspect_bypass,
    inspect_osdata, load_targets, remove_osdata, restore_bypass, windows_drive_roots, BypassState,
    OsdataState,
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
    match arguments.as_slice() {
        [operation] if operation == "run" => run_guided(),
        [operation] if operation == "probe-osdata" => {
            let windows_root = find_single_windows_installation(windows_drive_roots())?;
            print_result("probe-osdata", inspect_osdata(&windows_root)?);
            Ok(())
        }
        [operation, confirmation] if operation == "remove-osdata" && confirmation == "--apply" => {
            let windows_root = find_single_windows_installation(windows_drive_roots())?;
            print_result("remove-osdata", remove_osdata(&windows_root)?);
            Ok(())
        }
        [operation, flag] if operation == "bypass" && flag == "--apply" => {
            let windows_root = find_single_windows_installation(windows_drive_roots())?;
            print_bypass("bypass-apply", apply_bypass(&windows_root)?);
            Ok(())
        }
        [operation, flag] if operation == "bypass" && flag == "--restore" => {
            let windows_root = find_single_windows_installation(windows_drive_roots())?;
            print_bypass("bypass-restore", restore_bypass(&windows_root)?);
            Ok(())
        }
        [operation] if operation == "bypass" => {
            let windows_root = find_single_windows_installation(windows_drive_roots())?;
            print_bypass("bypass-inspect", inspect_bypass(&windows_root)?);
            Ok(())
        }
        _ => Err(
            "usage: meow-winpe-maintenance <run|probe-osdata|remove-osdata --apply|bypass [--apply|--restore]>"
                .into(),
        ),
    }
}

/// Guided flow driven by the host-generated TARGETS.JSON on the maintenance
/// CD: verifies the host preflight against what the guest sees, then removes
/// a leftover OSDATA node. The Utilman bypass stays an explicit subcommand so
/// it is never applied silently.
fn run_guided() -> Result<(), Box<dyn std::error::Error>> {
    match load_targets()? {
        Some((path, targets)) => {
            println!(
                "targets={} installs={}",
                path.display(),
                targets.installs.len()
            );
            for install in &targets.installs {
                println!(
                    "expected_install=P{} osdata={} bypass_available={}",
                    install.partition_index,
                    install.osdata_present,
                    install.utilman_bypass_available
                );
            }
        }
        None => println!("targets=absent"),
    }
    let windows_root = find_single_windows_installation(windows_drive_roots())?;
    println!("windows_root={}", windows_root.display());
    print_result("probe-osdata", inspect_osdata(&windows_root)?);
    print_result("remove-osdata", remove_osdata(&windows_root)?);
    print_bypass("bypass-inspect", inspect_bypass(&windows_root)?);
    Ok(())
}

fn print_result(operation: &str, state: OsdataState) {
    println!("operation={operation}");
    println!("result={state:?}");
}

fn print_bypass(operation: &str, state: BypassState) {
    println!("operation={operation}");
    println!("result={state:?}");
}
