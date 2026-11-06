# Worker shutdown

Upstream package: fjall 3.1.13, crates.io archive SHA-256
`6e98f7212d748b723dcfbd43d5f166958945ad2c05ad4537fb99bcda42ad8a96`.

`DatabaseInner::drop` waits for the worker activity counter. The counter guard
can be released before the closure's captured `WorkerState`, which owns a
supervisor and its file handles. Dropping a `JoinHandle` detaches the thread.
An immediate Windows directory publication can consequently fail with access
denied after database destruction returns.

`WorkerPool::drop` joins every worker after the database stop sequence. This
ensures worker-owned resources are released before database destruction returns.
Verification uses the actual database and process-termination publication tests
in `crates/persistence-fjall/tests/`.
