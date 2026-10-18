//! Explicit local-tool comparison; private evidence is never a CI prerequisite.
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE and FORENSICS_TSK_IMG_CAT"]
fn tsk_logical_sector_ranges_match_independent_e01_readers() {
    let fixture =
        std::env::var_os("FORENSICS_E01_FIXTURE").expect("explicit FORENSICS_E01_FIXTURE required");
    let executable =
        std::env::var_os("FORENSICS_TSK_IMG_CAT").expect("explicit FORENSICS_TSK_IMG_CAT required");
    let mut reader = image_e01::E01Reader::open(Path::new(&fixture)).unwrap();
    let size = reader.seek(SeekFrom::End(0)).unwrap();
    assert!(size >= 65536, "fixture needs at least 128 logical sectors");
    // This TSK CLI rejects an explicit start of zero; compare zero-based sectors >= 1.
    for start in [1, 63, (size / 512).saturating_sub(32).max(1)] {
        let output = tempfile::NamedTempFile::new().unwrap();
        let mut child = Command::new(&executable)
            .args([
                "-i",
                "ewf",
                "-b",
                "512",
                "-s",
                &start.to_string(),
                "-e",
                &(start + 31).to_string(),
            ])
            .arg(&fixture)
            .stdout(Stdio::from(output.reopen().unwrap()))
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("TSK bounded comparison timed out");
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        assert!(status.success(), "TSK could not decode this fixture");
        assert_eq!(output.as_file().metadata().unwrap().len(), 16384);
        let expected = std::fs::read(output.path()).unwrap();
        let mut actual = vec![0; expected.len()];
        reader.seek(SeekFrom::Start(start * 512)).unwrap();
        reader.read_exact(&mut actual).unwrap();
        assert_eq!(actual, expected, "logical byte mismatch at sector {start}");
    }
}
