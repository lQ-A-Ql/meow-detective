//! CI validates the actual signed release artifact without installing or executing it.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 2 {
        return Err("expected executable and version".into());
    }
    let executable = std::path::Path::new(&arguments[0]);
    let signature = std::fs::read_to_string(format!("{}.sig", executable.display()))?;
    portable_updater::validate_candidate(
        executable,
        &signature,
        portable_updater::PUBLIC_KEY,
        &arguments[1],
        portable_updater::CASE_SCHEMA,
    )?;
    println!("Signed portable executable and embedded version/schema verified");
    Ok(())
}
