use sha2::{Digest, Sha256};
fn main() {
    println!("cargo:rerun-if-env-changed=MEOW_UPDATER_PUBLIC_KEY");
    let directory = std::path::Path::new("../persistence-sqlite/src/migrations/scripts");
    let mut paths = std::fs::read_dir(directory)
        .expect("migration directory")
        .map(|entry| entry.expect("migration entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "sql"))
        .collect::<Vec<_>>();
    paths.sort();
    let mut digest = Sha256::new();
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        digest.update(
            path.file_name()
                .expect("script name")
                .to_string_lossy()
                .as_bytes(),
        );
        digest.update([0]);
        digest.update(
            std::fs::read_to_string(path)
                .expect("migration text")
                .replace("\r\n", "\n")
                .as_bytes(),
        );
        digest.update([0]);
    }
    println!("cargo:rustc-env=MEOW_CASE_SCHEMA={:x}", digest.finalize());
}
