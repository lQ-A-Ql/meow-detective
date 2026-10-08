pub fn filesystem(image: Vec<u8>) -> (fs_ntfs::NtfsReader, tempfile::TempDir) {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("wof.raw");
    std::fs::write(&path, image).unwrap();
    (
        fs_ntfs::NtfsReader::open(
            Box::new(evidence_core::RawImageReader::open(&path).unwrap()),
            0,
        )
        .unwrap(),
        temporary,
    )
}
