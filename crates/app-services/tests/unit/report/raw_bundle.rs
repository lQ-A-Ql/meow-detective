use super::*;
use std::io::{self, Read};

struct FailingReader {
    emitted: bool,
}

impl Read for FailingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.emitted {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "simulated evidence read failure",
            ));
        }
        self.emitted = true;
        let bytes = b"partial";
        buffer[..bytes.len()].copy_from_slice(bytes);
        Ok(bytes.len())
    }
}

#[test]
fn copy_failure_removes_partial_and_temporary_exports() {
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("evidence.bin");
    let result = copy_and_hash(&mut FailingReader { emitted: false }, &destination);

    assert!(result.is_err());
    assert!(!destination.exists());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
