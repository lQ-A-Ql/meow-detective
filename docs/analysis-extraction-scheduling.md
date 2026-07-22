# Analysis Extraction Scheduling

This document defines the production scheduling contract for Windows and Linux
data-source analysis. Both platforms use the same application-service runner
and the same bounded scheduler. Platform analyzers only select capabilities and
candidate policies; they do not own a separate execution loop.

## Scope

The scheduler applies to `run_source_analysis_extraction` and its progress
variant. It is not a replacement for the evidence-classification command or
for test-only compatibility helpers in `artifact_service`. Those paths have
different contracts and must not be mistaken for the platform analysis runner.

## Execution model

```text
data-source extraction gate
    -> candidate discovery and checkpoint lookup
    -> coordinator-owned serial evidence reads
    -> bounded CPU-only parser workers
    -> sequence-ordered coordinator merge
    -> coordinator-owned SQLite persistence
```

### Cross-source isolation

Only one source-analysis run may hold the process-level extraction gate at a
time. A waiting run periodically checks cancellation and emits a waiting
progress update. This prevents two E01/LVM/XFS readers from competing for the
same storage device while still allowing an individual source to use bounded
CPU parallelism.

### Within-source parallelism

The coordinator reads each candidate before dispatching it. Parser workers
receive owned byte buffers or registry-preloaded data and never access the
evidence reader or SQLite connection. The queue is bounded by the selected
worker budget and by an in-flight byte budget:

- normal mode: process worker budget, 256 MiB in-flight bytes;
- memory-throttled mode: one worker, 128 MiB in-flight bytes;
- worker count comes from the shared six-worker CPU policy and is additionally
  bounded by a conservative 512 MiB reservation per worker below the RSS soft
  limit;
- a candidate larger than the byte budget is processed as one bounded item,
  never duplicated across workers.

Completed results carry their input sequence. The coordinator stores out of
order results in a bounded pending map and applies them only in input order.
This keeps artifact/timeline output deterministic and makes SQLite writes
single-threaded.

## Platform contract

Windows and Linux both follow the exact scheduling stages above. The only
platform-specific behavior is:

- capability selection and platform validation;
- candidate classification and read-limit policy;
- parser dispatch for the owned input bytes;
- section labels and progress metadata.

Linux sections are therefore not launched as competing source-level jobs. They
are processed through one source run, with serial evidence reads and bounded
CPU parsing. A second data source waits at the extraction gate.

## Cancellation and failure behavior

Cancellation is checked before discovery, while waiting for the gate, before
and after reads, and in parser workers. A worker panic is converted into a
typed extraction error. A failed read becomes a candidate warning and does not
silently fabricate parser output. Persistence remains on the coordinator
thread so a parser worker cannot mutate SQLite state.

## Observability

The runner emits scheduler policy, candidate inventory, waiting time, current
candidate, read progress, section completion, and persistence phases. The
heartbeat includes submitted/completed counts, active in-flight items, byte
budget usage, worker budget, and current RSS. Progress is a report of actual
coordinator and parser state; it is not a time-based or synthetic percentage.

## Compatibility boundary

The public command names and DTO shapes remain unchanged. The progress-enabled
command is a thin transport adapter over the same service runner. The legacy
`artifact_service::run_extractors_parallel` helper remains test-only/compatibility
surface until a separately reviewed migration; it is not used by the Windows or
Linux source-analysis command and must not be used to infer production
scheduling behavior.

## Verification

The required checks are:

1. `cargo test -p app-services --lib analysis_service::extraction::scheduler`
2. `cargo test -p app-services --test analysis_platform_orchestration`
3. `cargo fmt --all -- --check`
4. `cargo clippy -p app-services --all-targets -- -D warnings`
5. Windows and Linux real-sample extraction with source runs executed
   serially and output counts compared with the established baselines.

The scheduler unit tests cover parallel parser execution, deterministic
ordered application, memory throttling, worker panic conversion, and gate
serialization.
