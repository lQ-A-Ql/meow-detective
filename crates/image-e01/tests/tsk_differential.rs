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
    let started = Instant::now();
    let reader = image_e01::E01Reader::open(Path::new(&fixture)).unwrap();
    let plan = reader.open_plan().unwrap();
    let size = plan.logical_size();
    println!(
        "E01 plan openMs={} metadataBytes={} chunks={} mirroredTables={}",
        started.elapsed().as_millis(),
        plan.metadata_bytes(),
        plan.chunk_count(),
        plan.mirrored_table_count()
    );
    let mut readers = (0..4)
        .map(|_| plan.open_reader().unwrap())
        .collect::<Vec<_>>();
    assert!(size >= 65536, "fixture needs at least 128 logical sectors");
    let mut starts = vec![
        0,
        1,
        63,
        (size / 2 / 512).saturating_sub(16),
        (size / 512).saturating_sub(32),
    ];
    let mut boot = [0; 512];
    readers[0].read_exact_at(0, &mut boot).unwrap();
    if &boot[3..11] == b"NTFS    " {
        let bps = u16::from_le_bytes(boot[11..13].try_into().unwrap()) as u64;
        let sectors = u64::from_le_bytes(boot[40..48].try_into().unwrap());
        if let Some(end) = sectors.checked_mul(bps).map(|bytes| bytes / 512) {
            if end > 16 && end + 16 < size / 512 {
                starts.push(end - 16);
                starts.push((end / 2).saturating_sub(16));
            }
        }
    }
    starts.sort_unstable();
    starts.dedup();
    for start in starts {
        let output = tempfile::NamedTempFile::new().unwrap();
        let mut command = Command::new(&executable);
        command.args(["-i", "ewf", "-b", "512", "-e", &(start + 31).to_string()]);
        // This TSK CLI rejects an explicit -s 0; omission means sector zero.
        if start > 0 {
            command.args(["-s", &start.to_string()]);
        }
        let mut child = command
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
        for reader in &mut readers {
            reader.seek(SeekFrom::Start(start * 512)).unwrap();
            reader.read_exact(&mut actual).unwrap();
            assert_eq!(actual, expected, "logical byte mismatch at sector {start}");
        }
        println!(
            "TSK oracle matched sector={start} bytes={} readers={}",
            expected.len(),
            readers.len()
        );
    }
}
