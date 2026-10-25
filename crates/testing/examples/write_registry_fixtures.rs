use std::{fs, io};
use testing::{builders::registry, fixtures};

fn main() -> io::Result<()> {
    fs::write(
        fixtures::tiny_registry_system_hive(),
        registry::synthetic_system_hive(),
    )?;
    fs::write(
        fixtures::tiny_registry_software_hive(),
        registry::synthetic_software_hive(),
    )?;
    Ok(())
}
