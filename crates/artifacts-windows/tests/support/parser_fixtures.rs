use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

pub fn bytes(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/windows-parsers")
        .join(name);
    let file = File::open(path).expect("open public parser fixture");
    let mut reader: Box<dyn Read> = if name.ends_with(".gz") {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut data = Vec::new();
    reader
        .read_to_end(&mut data)
        .expect("read public parser fixture");
    data
}
