use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

#[test]
fn public_parser_fixtures_match_the_recorded_sources() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/windows-parsers");
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("expected.json")).expect("read parser fixture manifest"),
    )
    .expect("decode parser fixture manifest");
    let samples = manifest["samples"].as_array().expect("fixture samples");
    assert_eq!(samples.len(), 9);
    for sample in samples {
        let name = sample["file"].as_str().expect("sample filename");
        let bytes = fs::read(root.join(name)).expect("read public sample");
        assert_eq!(
            hex::encode(Sha256::digest(&bytes)),
            sample["sha256"].as_str().expect("sample digest"),
            "source bytes for {name}",
        );
        if let Some(expected) = sample["uncompressedSha256"].as_str() {
            let mut decoded = Vec::new();
            flate2::read::GzDecoder::new(bytes.as_slice())
                .read_to_end(&mut decoded)
                .expect("decompress public sample");
            assert_eq!(
                hex::encode(Sha256::digest(&decoded)),
                expected,
                "decoded {name}"
            );
        }
    }
}
