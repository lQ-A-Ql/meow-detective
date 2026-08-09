use std::path::{Path, PathBuf};
use std::process::ExitCode;

use winpe_maintenance::{
    apply_bypass, crosscheck_install, ensure_winpe_runtime, find_single_windows_installation,
    inspect_bypass, inspect_osdata, load_targets, remove_osdata, restore_bypass, split_drive_flag,
    utilman_bypass_available, windows_drive_roots, BypassState, MaintenanceError,
    MaintenanceTargets, OsdataState,
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
    let (arguments, drive) = split_drive_flag(std::env::args().skip(1))?;
    match arguments.as_slice() {
        [operation] if operation == "run" => run_guided(drive.as_deref()),
        [operation] if operation == "probe-osdata" => {
            let windows_root = resolve_windows_root(drive.as_deref())?;
            print_result("probe-osdata", inspect_osdata(&windows_root)?);
            Ok(())
        }
        [operation, confirmation] if operation == "remove-osdata" && confirmation == "--apply" => {
            let windows_root = resolve_windows_root(drive.as_deref())?;
            print_result("remove-osdata", remove_osdata(&windows_root)?);
            Ok(())
        }
        [operation, flag] if operation == "bypass" && flag == "--apply" => {
            let windows_root = resolve_windows_root(drive.as_deref())?;
            print_bypass("bypass-apply", apply_bypass(&windows_root)?);
            Ok(())
        }
        [operation, flag] if operation == "bypass" && flag == "--restore" => {
            let windows_root = resolve_windows_root(drive.as_deref())?;
            print_bypass("bypass-restore", restore_bypass(&windows_root)?);
            Ok(())
        }
        [operation] if operation == "bypass" => {
            let windows_root = resolve_windows_root(drive.as_deref())?;
            print_bypass("bypass-inspect", inspect_bypass(&windows_root)?);
            Ok(())
        }
        _ => Err(
            "usage: meow-winpe-maintenance <run|probe-osdata|remove-osdata --apply|bypass [--apply|--restore]> [--drive <letter>]"
                .into(),
        ),
    }
}

/// `--drive` selects the installation root explicitly; without it,
/// auto-detection applies and supports exactly one offline Windows
/// installation.
fn resolve_windows_root(drive: Option<&Path>) -> Result<PathBuf, MaintenanceError> {
    match drive {
        Some(root) => Ok(root.to_path_buf()),
        None => find_single_windows_installation(windows_drive_roots()),
    }
}

/// Guided flow driven by the host-generated TARGETS.JSON on the maintenance
/// CD: cross-checks the host preflight against what the guest sees (aborting
/// on any mismatch), then removes a leftover OSDATA node. The Utilman bypass
/// stays an explicit subcommand so it is never applied silently.
fn run_guided(drive: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    let targets = load_targets()?;
    match &targets {
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
    let windows_root = resolve_windows_root(drive)?;
    println!("windows_root={}", windows_root.display());
    let osdata = inspect_osdata(&windows_root)?;
    print_result("probe-osdata", osdata);
    if let Some((_, targets)) = &targets {
        crosscheck(targets, osdata, utilman_bypass_available(&windows_root))?;
    }
    print_result("remove-osdata", remove_osdata(&windows_root)?);
    print_bypass("bypass-inspect", inspect_bypass(&windows_root)?);
    Ok(())
}

/// A mismatch means the guest booted into an environment the host did not
/// expect, so the guided run aborts before any write. Cross-checking needs a
/// single manifest entry: the guest cannot map a drive letter back to a host
/// partition index, so a multi-install manifest is reported and skipped.
fn crosscheck(
    targets: &MaintenanceTargets,
    osdata: OsdataState,
    bypass_available: bool,
) -> Result<(), MaintenanceError> {
    let [install] = targets.installs.as_slice() else {
        println!("crosscheck=SKIPPED installs={}", targets.installs.len());
        return Ok(());
    };
    let mismatches = crosscheck_install(install, osdata != OsdataState::Missing, bypass_available);
    if mismatches.is_empty() {
        println!("crosscheck=OK");
        return Ok(());
    }
    for mismatch in &mismatches {
        println!(
            "crosscheck=MISMATCH {} expected={} observed={}",
            mismatch.field, mismatch.expected, mismatch.observed
        );
    }
    Err(MaintenanceError::CrosscheckMismatch)
}

fn print_result(operation: &str, state: OsdataState) {
    println!("operation={operation}");
    println!("result={state:?}");
}

fn print_bypass(operation: &str, state: BypassState) {
    println!("operation={operation}");
    println!("result={state:?}");
}
