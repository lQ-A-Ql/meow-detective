use std::{env, fs, path::PathBuf};

use testing::builders::registry;

fn main() -> anyhow::Result<()> {
    let repo_root = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(testing::fixtures::repo_root);
    let config_dir = repo_root.join("testdata/fixtures/tiny/logical/Windows/System32/config");
    fs::create_dir_all(&config_dir)?;

    let system_path = config_dir.join(registry::SYSTEM_HIVE_NAME);
    let software_path = config_dir.join(registry::SOFTWARE_HIVE_NAME);
    fs::write(&system_path, registry::synthetic_system_hive())?;
    fs::write(&software_path, registry::synthetic_software_hive())?;

    println!(
        "Wrote {} ({} bytes)",
        system_path.display(),
        fs::metadata(&system_path)?.len()
    );
    println!(
        "Wrote {} ({} bytes)",
        software_path.display(),
        fs::metadata(&software_path)?.len()
    );
    Ok(())
}
