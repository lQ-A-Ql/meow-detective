# Meow~Detective local patch

Base: `iscsi-target 1.0.0` from crates.io.

The patch is intentionally limited to forensic read-only operation:

- adds `ScsiBlockDevice::is_read_only()`;
- rejects SCSI writes with `DATA PROTECT / WRITE PROTECTED`;
- sets the MODE SENSE write-protect bit;
- accepts a pre-bound TCP listener for race-free loopback port allocation.
- emits RFC 7143 compliant Data-In underflow/overflow residual flags and counts;
- stores Data-In status independently from the residual-count field;
- records connection-processing errors instead of silently discarding them.

The Data-In compatibility changes follow RFC 7143 sections 11.4.5 and 11.7.3.
They are covered by a wire-header regression test and an elevated Microsoft
iSCSI Initiator interoperability test.

No evidence parsing or application logic is carried by this vendored crate.
