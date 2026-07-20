# Import Scheduling Contract

This document is the authoritative contract for import-time scheduling across
Windows, Linux, and Linux/PVE cluster sources. It describes resource policy,
not filesystem parsing. Evidence readers, staging, and SQLite persistence keep
their existing ownership in the import pipeline.

## Baseline and scope

Windows is the baseline for the scheduler: one source may use a bounded CPU
budget of at most four workers, while evidence reads remain subject to the
reader safety rules. Linux single-source imports use the same
`ImportWorkload::SingleSource` policy; they do not have a separate Linux
thread pool or a separate progress state machine. This means:

- partition enumeration and post-import analysis use the same worker-limit
  resolution for Windows and Linux;
- an E01-backed partition reader is still forced to one actual enumeration
  worker, even when the configured upper bound is higher;
- CPU-only parser and analysis work may use the remaining bounded worker
  budget; and
- top-level import jobs remain serialized by the existing evidence-I/O gate.

The process-level gate is intentional. It prevents unrelated E01/LVM/XFS
readers from competing for the same physical evidence volume. It does not
serialize members inside one Linux cluster import.

## Policy

`crates/app-services/src/import_scheduler.rs` owns the policy and admission
controller. The defaults are:

| Workload | Active source limit | Per-source import workers | Per-source analysis workers | CPU budget | Memory reservation |
|---|---:|---:|---:|---:|---:|
| Windows or Linux single source | 1 | auto, up to 4 | auto, up to 4 | 4 | 4096 MiB |
| Linux cluster | 2 members | auto, up to 2 | auto, up to 2 | 4 total | 4096 MiB total, normally 2048 MiB/member |

The settings values are upper bounds, not a request to exceed the global
budget. An explicit value is clamped to the scheduler capacity; an empty
value selects automatic scheduling. A cluster with six members creates a
bounded set of member workers, but only members admitted by the controller
read evidence at the same time.

Admission is weighted by both CPU and reserved memory. `ImportPermit` releases
both reservations on every exit path, including cancellation and panic unwind.
The controller also records active source count and peak active source count
for diagnostics. RSS is sampled as a safety signal: a source is not admitted
while an active import is already at the global soft threshold, and the
analysis pipeline retains its existing hard-limit cancellation behavior.

## Cluster execution

The Linux cluster runner owns one parent job and creates one child job per
member so each member can report its own source-scoped progress and outcome.
The child jobs are coordinated as follows:

1. The parent registers the cluster and writes its manifest once.
2. Member jobs are created in deterministic member-index order.
3. Member workers wait for scheduler admission, open an independent case
   connection, and execute the normal single-source import pipeline with the
   scheduler-selected worker limits.
4. The coordinator thread is the only writer of the parent cluster state and
   aggregate counts. Results are consumed until every spawned member has
   drained, even when one member fails or cancellation is requested.
5. A member that has already completed successfully remains `completed` when
   a later member is cancelled. Waiting or running members are marked
   `cancelled`; non-cancellation failures are marked `failed` and do not stop
   unrelated members.

Each member has its own `source.db`, staging database, and source-local index.
The scheduler never merges evidence data between members. Concurrent SQLite
writes target independent source databases; short control-record writes to
`app.db` use WAL and the configured busy timeout.

## Platform and phase boundary

Windows and Linux artifact extraction is not launched as one competing task
per section. A source runs one pipeline; its evidence reads and coordinator
merges stay ordered, and bounded CPU workers process owned data. Linux cluster
members are the only source-level parallel unit in this contract. BlueStore
metadata and Ceph/RBD derived processing retain their own phase boundaries and
are not silently multiplied by the import scheduler.

## Cancellation and failure

Cancellation is checked while waiting for admission and inside the existing
enumeration and analysis workers. A cancelled waiter exits without consuming a
permit. The cluster coordinator continues receiving results after cancellation
so all permits, connections, and child jobs drain before the parent returns.
Worker panics are converted to a member failure; they cannot strand the global
admission capacity.

## Verification matrix

The scheduler unit tests in
`crates/app-services/tests/import_scheduler.rs` verify:

- Windows/Linux single-source and cluster policies share the four-worker CPU
  cap;
- two low-weight cluster members can be active while total weight remains at
  or below four;
- memory and CPU reservations are released after normal completion;
- multiple waiting members cancel without leaking capacity; and
- panic unwind releases the permit.

The real PVE regression is opt-in and uses the existing desktop test harness:

```powershell
$env:FORENSICS_PVE_CLUSTER_ROOT = 'E:\pangushi\服务器'
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1
```

The run must record wall time, peak RSS, scheduler admission observations,
member ready/failed counts, and source-database isolation. It must not claim a
throughput improvement until the same fixture has a comparable serial baseline
under the same build and storage conditions.
