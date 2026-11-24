use super::*;
use crossbeam_channel::{bounded, Receiver, Sender};

pub(super) fn run<T: NativeWriteInput>(
    session: &SourceWriteSession,
    cancel: &AtomicBool,
    mut producer: impl FnMut() -> Result<Option<Vec<T>>>,
    route: impl Fn(&T) -> u64,
    encode: impl Fn(&SourceReadHandle, Vec<T>) -> Result<Vec<WriteAction>> + Sync,
) -> Result<()> {
    let result = std::thread::scope(|scope| {
        let mut senders = Vec::new();
        let mut workers = Vec::new();
        for worker in 0..session.config.workers {
            let (sender, receiver) = bounded(1);
            senders.push(sender);
            let encode = &encode;
            workers.push(
                std::thread::Builder::new()
                    .name(format!("fjall:application:{worker}"))
                    .spawn_scoped(scope, move || {
                        let result = work(session, cancel, worker, receiver, encode);
                        if result.is_err() {
                            session.fail();
                        }
                        result
                    })?,
            );
        }
        let produced = produce(session, cancel, &senders, &mut producer, &route);
        if produced.is_err() {
            session.fail();
        }
        drop(senders);
        let mut result = produced;
        for worker in workers {
            let outcome = worker
                .join()
                .map_err(|_| FjallStorageError::WriteWorkerTerminated)?;
            if result.is_ok()
                || (outcome.is_err()
                    && matches!(
                        result,
                        Err(FjallStorageError::WriteSessionFailed
                            | FjallStorageError::WriteWorkerTerminated)
                    ))
            {
                result = outcome;
            }
        }
        result
    });
    if result.is_err() {
        session.fail();
    }
    result
}

fn produce<T: NativeWriteInput>(
    session: &SourceWriteSession,
    cancel: &AtomicBool,
    senders: &[Sender<Vec<T>>],
    producer: &mut impl FnMut() -> Result<Option<Vec<T>>>,
    route: &impl Fn(&T) -> u64,
) -> Result<()> {
    while let Some(records) = producer()? {
        session.ensure_active(cancel)?;
        let mut groups = (0..senders.len()).map(|_| Vec::new()).collect::<Vec<_>>();
        let mut bytes = vec![0usize; senders.len()];
        for record in records {
            let worker = (route(&record) % senders.len() as u64) as usize;
            let weight = record.resident_bytes()?;
            if weight > session.config.batch_bytes {
                return Err(FjallStorageError::WriteSessionLimit);
            }
            if bytes[worker].saturating_add(weight) > session.config.batch_bytes / 3 {
                senders[worker]
                    .send(std::mem::take(&mut groups[worker]))
                    .map_err(|_| FjallStorageError::WriteWorkerTerminated)?;
                bytes[worker] = 0;
            }
            groups[worker].push(record);
            bytes[worker] += weight;
        }
        for (sender, group) in senders.iter().zip(groups) {
            if !group.is_empty() {
                sender
                    .send(group)
                    .map_err(|_| FjallStorageError::WriteWorkerTerminated)?;
            }
        }
    }
    Ok(())
}

fn work<T: NativeWriteInput>(
    session: &SourceWriteSession,
    cancel: &AtomicBool,
    worker: usize,
    receiver: Receiver<Vec<T>>,
    encode: &impl Fn(&SourceReadHandle, Vec<T>) -> Result<Vec<WriteAction>>,
) -> Result<()> {
    let reader = session.database.read_handle();
    let mut buffered = Vec::new();
    let mut bytes = 0usize;
    for records in receiver {
        session.ensure_active(cancel)?;
        for record in records {
            let weight = record.resident_bytes()?;
            if !buffered.is_empty() && bytes.saturating_add(weight) > session.config.batch_bytes / 3
            {
                session.ingest(
                    worker,
                    encode(&reader, std::mem::take(&mut buffered))?,
                    cancel,
                )?;
                bytes = 0;
            }
            bytes += weight;
            buffered.push(record);
        }
    }
    if !buffered.is_empty() {
        session.ingest(worker, encode(&reader, buffered)?, cancel)?;
    }
    Ok(())
}
