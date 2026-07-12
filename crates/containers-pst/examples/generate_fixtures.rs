use std::fs;
use std::path::PathBuf;

#[path = "../tests/unit/support/synthetic.rs"]
mod synthetic;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest_dir
        .join("../../testdata/fixtures/public-small/email")
        .canonicalize()
        .expect("fixture dir exists");

    let pst = synthetic::build_synthetic_pst_with_messages(1);
    fs::write(fixture_dir.join("synthetic.pst"), &pst).expect("write synthetic.pst");

    // OST uses the same binary layout in the current synthetic builder.
    fs::write(fixture_dir.join("synthetic.ost"), &pst).expect("write synthetic.ost");

    println!("Generated synthetic.pst and synthetic.ost in {fixture_dir:?}");
}
