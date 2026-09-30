use super::*;

#[test]
#[ignore = "requires compatible OpenCL GPU hardware and driver"]
fn opencl_kdf_matches_cpu_reference_and_cancel_releases_resources() {
    let runtime = GpuRuntime::new().expect("GPU driver");
    let hashes = [[0x01234567; 8], [0x89abcdef; 8], [0; 8]];
    runtime.self_test(&hashes[0]).expect("driver self-test");
    for salt in [[0u8; 16], [0x33; 16]] {
        for rounds in [0, 1, 17, 4097, ITERATIONS] {
            eprintln!("OpenCL reference comparison rounds={rounds}");
            let output = runtime
                .stretch_n(&hashes, &salt, &AtomicBool::new(false), rounds)
                .expect("GPU computation")
                .expect("not cancelled");
            for (index, initial) in hashes.iter().enumerate() {
                let mut reference = [0u8; 88];
                for (i, word) in initial.iter().enumerate() {
                    reference[32 + i * 4..36 + i * 4].copy_from_slice(&word.to_be_bytes());
                }
                reference[64..80].copy_from_slice(&salt);
                for count in 0..u64::from(rounds) {
                    reference[80..88].copy_from_slice(&count.to_le_bytes());
                    let hash = Sha256::digest(reference);
                    reference[..32].copy_from_slice(&hash);
                }
                assert_eq!(
                    output[index],
                    reference[..32],
                    "salt/round/candidate isolation"
                );
            }
        }
    }
    assert!(runtime
        .stretch(&hashes, &[0; 16], &AtomicBool::new(true))
        .expect("cancel")
        .is_none());
    let cancel = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let started = std::time::Instant::now();
        scope.spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            cancel.store(true, Ordering::Release);
        });
        assert!(runtime
            .stretch(&vec![hashes[0]; MAX_GPU_CANDIDATES], &[0; 16], &cancel)
            .expect("cancel during kernel")
            .is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    });
    assert!(matches!(GpuRuntime::new(), Err(GpuError::Busy)));
    drop(runtime);
    GpuRuntime::new().expect("permit released");
}
