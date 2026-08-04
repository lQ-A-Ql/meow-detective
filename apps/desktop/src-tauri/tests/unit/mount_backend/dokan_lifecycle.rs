use std::sync::{mpsc, Mutex};
use std::thread;

use widestring::U16CString;

use super::{DokanMount, MountPublication, StartupEvent};

#[test]
fn publication_reports_the_actual_mount_manager_path() {
    let (sender, receiver) = mpsc::sync_channel(1);
    let publication = MountPublication::new(sender);
    let mount_point = U16CString::from_str("Y:\\").expect("valid mount point");

    publication.publish(mount_point.as_ucstr());

    match receiver.recv().expect("publication event") {
        StartupEvent::Mounted(actual) => assert_eq!(actual.to_string_lossy(), "Y:\\"),
        StartupEvent::Failed(error) => panic!("unexpected publication failure: {error}"),
    }
}

#[test]
fn finished_worker_is_reported_as_inactive() {
    let worker = thread::spawn(|| Ok(()));
    while !worker.is_finished() {
        thread::yield_now();
    }
    let mount = DokanMount {
        mount_point: U16CString::from_str("Y:\\").expect("valid mount point"),
        join_handle: Mutex::new(Some(worker)),
    };

    let failure = mount
        .poll_exit()
        .expect("worker state must be readable")
        .expect("finished worker must be reported");

    assert!(failure.contains("exited unexpectedly"));
}
