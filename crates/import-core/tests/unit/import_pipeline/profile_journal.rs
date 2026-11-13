use crate::import_pipeline::{profile::Counters, ImportProfiler};
use std::{
    fs,
    process::{Child, Command},
    time::{Duration, Instant},
};
use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
};

fn scratch() -> std::path::PathBuf {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/import");
    fs::create_dir_all(&path).unwrap();
    path.canonicalize().unwrap()
}

struct RunningChild(Child);
impl Drop for RunningChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn completed_profile_survives_process_termination() {
    let root = tempfile::tempdir_in(scratch()).unwrap();
    let name = format!(
        "{}::journal_process_child",
        module_path!().split_once("::").unwrap().1
    );
    let mut command = Command::new(std::env::current_exe().unwrap());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let child = command
        .args(["--exact", &name, "--ignored", "--nocapture"])
        .env("IMPORT_PROFILE_CHILD_ROOT", root.path())
        .env("TEMP", root.path())
        .env("TMP", root.path())
        .spawn()
        .unwrap();
    let mut child = RunningChild(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.path().join("ready").exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "profile writer ended before handshake"
        );
        assert!(
            Instant::now() < deadline,
            "profile writer did not reach handshake"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let directory = fs::read_dir(root.path())
        .unwrap()
        .map(Result::unwrap)
        .find(|entry| entry.file_type().unwrap().is_dir())
        .unwrap()
        .path();
    let mut profiles = fs::read_dir(directory)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    profiles.sort();
    let report: transport::dto::import_benchmark::ImportBenchmarkReportDto =
        serde_json::from_slice(&fs::read(profiles.last().unwrap()).unwrap()).unwrap();
    let sample = report
        .samples
        .iter()
        .find(|sample| sample.phase == Phase::SourceIndexBuild)
        .unwrap();
    assert_eq!(sample.state, State::Completed);
    assert!(sample.elapsed_ns.is_some_and(|elapsed| elapsed > 0));
    assert_eq!(sample.logical_bytes_read, 1024 * 1024);
}

#[test]
#[ignore = "subprocess entry for completed_profile_survives_process_termination"]
fn journal_process_child() {
    let root = std::path::PathBuf::from(std::env::var_os("IMPORT_PROFILE_CHILD_ROOT").unwrap());
    let profiler = ImportProfiler::with_output_directory(&root).unwrap();
    let started = Instant::now();
    let payload = vec![0x5a; 1024 * 1024];
    let data = root.join("physical-records.bin");
    fs::write(&data, &payload).unwrap();
    assert_eq!(fs::read(data).unwrap(), payload);
    profiler.record(
        None,
        Phase::SourceIndexBuild,
        Some(started.elapsed()),
        State::Completed,
        Counters {
            bytes: payload.len() as u64,
            ..Default::default()
        },
    );
    fs::write(root.join("ready"), b"committed").unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn journal_io_failure_is_retained_and_destructors_do_not_retry() {
    use crate::import_pipeline::profile::{CleanupRecorder, PhaseRecorder};
    let root = tempfile::tempdir_in(scratch()).unwrap();
    let profiler = ImportProfiler::with_output_directory(root.path()).unwrap();
    let journal = fs::read_dir(root.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let retained = root.path().join("retained-journal");
    fs::rename(&journal, &retained).unwrap();
    fs::write(&journal, b"directory replaced by a file").unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut cleanup = CleanupRecorder::new(Some(profiler.clone()));
        cleanup.start();
        let mut phases = PhaseRecorder::new(Some(profiler.clone()), Some(0));
        let started = phases.start();
        let payload = root.path().join("payload.bin");
        fs::write(&payload, b"physical read").unwrap();
        assert_eq!(fs::read(payload).unwrap(), b"physical read");
        phases.record(Phase::MftRead, started, State::Completed);
    }));
    assert!(
        result.is_ok(),
        "journal I/O failure must not panic during Drop"
    );
    let first = profiler
        .output_error()
        .expect("journal failure must be queryable");
    fs::remove_file(&journal).unwrap();
    fs::rename(&retained, &journal).unwrap();
    let before = fs::read_dir(&journal).unwrap().count();
    assert_eq!(
        profiler.record_progress(&serde_json::json!({"event":"after-failure"})),
        Err(first.clone())
    );
    let failure = crate::import_pipeline::profile::measure_command(
        Some(&profiler),
        Phase::Checkpoint,
        || {
            fs::read(root.path().join("payload.bin"))
                .map_err(transport::CommandError::from_typed_service_error)
        },
    )
    .unwrap_err();
    assert_eq!(failure.code, "IMPORT_PROFILE_OUTPUT");
    assert_eq!(failure.category, "io");
    assert_eq!(profiler.check_output(), Err(first));
    assert_eq!(fs::read_dir(&journal).unwrap().count(), before);
}

#[test]
fn invalid_output_directory_returns_a_typed_error() {
    let root = tempfile::tempdir_in(scratch()).unwrap();
    let file = root.path().join("file");
    fs::write(&file, b"not a directory").unwrap();
    let failure = ImportProfiler::with_output_directory(file).unwrap_err();
    assert_eq!(
        transport::ServiceErrorCategory::code(&failure),
        Some("IMPORT_PROFILE_OUTPUT")
    );
}
