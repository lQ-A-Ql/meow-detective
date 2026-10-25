use super::{job::Job, pipes, BoltReadError};
use std::{
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use transport::dto::bolt_reader::*;

struct ChildGuard {
    child: Child,
    job: Option<Job>,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        drop(self.job.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn executable() -> Result<PathBuf, BoltReadError> {
    let path = match std::env::var_os("FORENSICS_BOLT_READER_EXE") {
        Some(path) => PathBuf::from(path),
        None => std::env::current_exe()?,
    };
    if !path.is_file() {
        return Err(BoltReadError::MissingWorker);
    }
    Ok(path)
}

pub(super) fn execute(
    request: &BoltReadRequestDto,
    cancel: &AtomicBool,
) -> Result<BoltReadReportDto, BoltReadError> {
    let executable = executable()?;
    let mut command = Command::new(&executable);
    command
        .arg("--internal-bolt-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(parent) = executable.parent() {
        command.current_dir(parent);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = ChildGuard {
        child: command.spawn()?,
        job: None,
    };
    child.job = Some(Job::attach(&child.child, &request.limits)?);
    let mut stdin = child
        .child
        .stdin
        .take()
        .ok_or(BoltReadError::WorkerFailed)?;
    serde_json::to_writer(&mut stdin, request)?;
    stdin.flush()?;
    drop(stdin);
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout = pipes::capture(
        child
            .child
            .stdout
            .take()
            .ok_or(BoltReadError::WorkerFailed)?,
        request.limits.max_output_bytes,
        overflow.clone(),
    );
    let stderr = pipes::capture(
        child
            .child
            .stderr
            .take()
            .ok_or(BoltReadError::WorkerFailed)?,
        16 * 1024,
        overflow.clone(),
    );
    let status = wait(
        &mut child.child,
        request.limits.max_runtime_millis,
        cancel,
        &overflow,
    )?;
    if let Some(job) = child.job.as_mut() {
        job.completed();
    }
    drop(child.job.take());
    let output = stdout.join().map_err(|_| BoltReadError::WorkerFailed)??;
    stderr.join().map_err(|_| BoltReadError::WorkerFailed)??;
    if status.code() == Some(12) {
        return Err(BoltReadError::OutputLimit);
    }
    if !status.success() {
        return Err(BoltReadError::WorkerFailed);
    }
    let response: BoltReadResponseDto = serde_json::from_slice(&output)?;
    if response.protocol_version != BOLT_READER_PROTOCOL_VERSION
        || response.request_id != request.request_id
    {
        return Err(BoltReadError::InvalidResponse);
    }
    match response.result {
        BoltReadResultDto::Complete { report } => {
            super::validation::report(request, &report)?;
            Ok(report)
        }
        BoltReadResultDto::Failed { .. } => Err(BoltReadError::Decode),
    }
}

fn wait(
    child: &mut Child,
    milliseconds: u64,
    cancel: &AtomicBool,
    overflow: &AtomicBool,
) -> Result<std::process::ExitStatus, BoltReadError> {
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(BoltReadError::Cancelled);
        }
        if overflow.load(Ordering::Acquire) {
            return Err(BoltReadError::OutputLimit);
        }
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if started.elapsed() >= Duration::from_millis(milliseconds) {
            return Err(BoltReadError::Timeout);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
#[path = "../../tests/unit/bolt_reader_service/process.rs"]
mod tests;
