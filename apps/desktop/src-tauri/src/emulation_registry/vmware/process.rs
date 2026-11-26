use std::{path::Path, process::Command};

pub(super) fn vmrun_command(path: &Path) -> Command {
    let command = Command::new(path);
    #[cfg(windows)]
    let command = {
        use std::os::windows::process::CommandExt;
        let mut hidden = command;
        hidden.creation_flags(0x0800_0000);
        hidden
    };
    command
}
