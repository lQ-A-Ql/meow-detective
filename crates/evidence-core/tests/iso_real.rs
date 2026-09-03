use evidence_core::{FileSystemReader, Iso9660Reader};
use std::path::PathBuf;

#[test]
#[ignore = "requires FORENSICS_ISO_FIXTURE read-only ISO9660/Joliet sample"]
fn opens_real_iso_and_enumerates_root() {
    let path = std::env::var_os("FORENSICS_ISO_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_ISO_FIXTURE");
    let filesystem = Iso9660Reader::open(&path).expect("open ISO fixture");
    let root = filesystem.root().expect("read ISO root");
    let children = filesystem.list_children("").expect("enumerate ISO root");
    assert!(root.is_dir);
    assert!(!children.is_empty());
}
