#![cfg_attr(windows, windows_subsystem = "windows")]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    if std::env::args_os().any(|argument| argument == "--internal-bolt-worker") {
        std::process::exit(containers_bolt::worker_exit_code());
    }
    if portable_updater::run_if_requested() {
        return;
    }
    forensics_desktop_lib::run();
}
