use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub(super) const SAMPLE_INTERVAL_MS: u64 = 2;

pub(super) struct MemorySampler {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<u64>>,
    start_rss: u64,
}

impl MemorySampler {
    pub fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let start_rss = app_services::runtime_resources::current_rss_mb();
        assert!(start_rss > 0, "process RSS measurement is unavailable");
        let worker = thread::spawn(move || {
            let mut peak = start_rss;
            while !worker_stop.load(Ordering::Acquire) {
                peak = peak.max(app_services::runtime_resources::current_rss_mb());
                thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
            }
            peak.max(app_services::runtime_resources::current_rss_mb())
        });
        Self {
            stop,
            worker: Some(worker),
            start_rss,
        }
    }

    pub fn finish(mut self) -> serde_json::Value {
        let end_rss = app_services::runtime_resources::current_rss_mb();
        self.stop.store(true, Ordering::Release);
        let peak = self
            .worker
            .take()
            .unwrap()
            .join()
            .expect("memory sampler thread");
        serde_json::json!({
            "rssAtStartMb": self.start_rss,
            "rssAtEndMb": end_rss,
            "sampledPeakRssMb": peak.max(end_rss),
            "processLifetimePeakRssMb": app_services::runtime_resources::peak_rss_mb(),
            "sampleIntervalMs": SAMPLE_INTERVAL_MS,
        })
    }
}

impl Drop for MemorySampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("memory sampler thread");
        }
    }
}
