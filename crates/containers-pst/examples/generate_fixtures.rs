use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest_dir
        .join("../../testdata/fixtures/public-small/email")
        .canonicalize()
        .expect("fixture dir exists");

    let pst = containers_pst::pst::build_synthetic_pst();
    fs::write(fixture_dir.join("synthetic.pst"), &pst).expect("write synthetic.pst");

    // OST uses the same binary layout in the current synthetic builder.
    fs::write(fixture_dir.join("synthetic.ost"), &pst).expect("write synthetic.ost");

    println!("Generated synthetic.pst and synthetic.ost in {fixture_dir:?}");
}
