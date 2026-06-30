use containers_pst::pst::{build_synthetic_pst_with_messages, PstReader};
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from("testdata/fixtures/public-medium/email/medium-pst");
    fs::create_dir_all(&out_dir).unwrap();

    let pst_path = out_dir.join("medium.pst");
    let ost_path = out_dir.join("medium.ost");

    let pst_data = build_synthetic_pst_with_messages(10);
    fs::write(&pst_path, &pst_data).unwrap();

    let ost_data = build_synthetic_pst_with_messages(10);
    fs::write(&ost_path, &ost_data).unwrap();

    // Sanity-check that the reader can enumerate all messages.
    let reader = PstReader::open(&pst_path).unwrap();
    let messages = reader.read_messages().unwrap();
    assert_eq!(messages.len(), 10, "expected 10 synthetic messages");

    println!(
        "Generated {} ({} bytes)",
        pst_path.display(),
        pst_data.len()
    );
    println!(
        "Generated {} ({} bytes)",
        ost_path.display(),
        ost_data.len()
    );
}
