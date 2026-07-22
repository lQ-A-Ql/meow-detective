# PVE / Linux Cluster Parsing Design

## Summary

This document defines the engineering boundary for PVE / Linux cluster work.
The parser baseline remains single-disk Linux server parsing: E01/Raw ->
partition table -> LVM direct linear/striped LV -> XFS -> file tree, preview,
and Linux artifact extraction.

Cluster import modeling is now enabled as a Stage 1 capability. The UI can
submit a Linux cluster folder, the backend scans nested image members,
registers a case-level cluster record, writes a manifest, and imports members
through the shared bounded scheduler (at most two active members by default),
with each member image written to its own source database. A failed member is recorded
without aborting later members; the final cluster and job remain failed with
partial counts when any member fails. Cluster-level parsing,
PVE thin-pool reconstruction, VM disk reconstruction, and cross-node analysis
remain non-executing future stages.

## Development Baseline

- Current accepted path: single-disk XFS-on-LVM sample `D:\獬豸杯\检材3.E01`.
- Required result for this milestone:
  - enumerate the complete root LV file tree for the single-disk sample;
  - preview arbitrary readable files through the existing viewer path;
  - extract key Linux forensic artifacts from the parsed tree.
- Existing reusable pieces:
  - `fs-lvm` for PV label, VG metadata, and direct LV extent mapping;
  - `fs-xfs` for XFS directory and file reads;
  - `app-services::datasource_service::expand_lvm_pool_candidates*` for LVM
    expansion;
  - `app-services::analysis_service` Linux extraction for system, auth, shell,
    cron, journal, and PVE config artifacts.

## Development Boundary

### In Scope Now

- Keep single-source, single-disk Linux parsing stable.
- Keep LVM pool partitions redirected/hidden after successful LV expansion.
- Keep preview and artifact extraction working for XFS files inside direct LVs.
- Provide a typed cluster parsing service boundary and first-pass cluster
  import modeling.
- Document the future PVE cluster architecture and test matrix.

### Out of Scope Now

- PVE cluster semantic analysis execution.
- Multi-node correlation of `/etc/pve` state.
- LVM thin-pool block mapping.
- LVM cache, RAID, snapshot, VDO, writecache reconstruction.
- Partial/degraded VG activation.
- PVE VM disk reconstruction.

## Interface Boundary

`cluster_service` owns the future cluster parsing boundary:

- `ClusterEvidenceSource`
  - `source_path`
  - `source_kind`
- `ClusterParseRequest`
  - list of evidence sources
- `ClusterParsePlan`
  - source count
  - `supported_now = false`
  - explicit boundary enum
- `plan_cluster_parse(request)`
  - validates that a cluster requires multiple sources;
  - returns a non-executing plan.
- `parse_cluster(request)`
  - semantic cluster reconstruction currently returns `Unsupported`.
- `plan_linux_cluster_import(root_path, profile)`
  - scans first-level supported image members;
  - returns a cluster import plan and manifest path.

The registered Tauri import command accepts `sourceKind=linuxCluster` for the
import-modeling stage only. It does not execute semantic PVE reconstruction.

## Stage Design

### Stage 0 - Current Milestone: XFS Single Disk

Goal: keep `检材3` working end to end.

Tasks:
- validate LVM direct LV expansion;
- validate XFS root tree enumeration;
- validate arbitrary file preview;
- validate Linux artifact extraction;
- keep semantic cluster parsing non-executing.

Expected result:
- single-disk Linux server evidence is usable for investigation;
- cluster semantic parsing remains impossible to trigger accidentally.

### Stage 1 - Cluster Evidence Set Modeling

Goal: model multi-source evidence without parsing thin pools.

Tasks:
- create case-level evidence set entities;
- group related E01/Raw sources by investigator folder selection;
- persist cluster manifest and source membership metadata;
- import each member through the existing single-source pipeline.

Expected result:
- the app can import a PVE/Linux cluster evidence set as grouped member source
  databases without attempting thin-pool or VM reconstruction.

### Stage 2 - PVE Host Filesystem Coverage

Goal: parse each node host OS independently.

Status (2026-07-10): host filesystem baseline complete for the private PVE sample.

Tasks:
- run the existing single-disk pipeline per source;
- extract `/etc/pve`, `/etc/corosync`, `/var/log/pve*`, systemd, auth, shell,
  and package artifacts;
- normalize node identity from hostname, machine-id, corosync config, and PVE
  config.

Expected result:
- investigators can compare host-level config and logs across nodes even before
  VM disk reconstruction is supported.
- all three `disk01` images expose `pve/root` as 64-bit EXT4; a representative
  member imports 56,471 files and 5,931 directories and supports `FileEntryId`
  preview of `/var/lib/pve-cluster/config.db`.

### Stage 3 - LVM Thin Metadata Research Spike

Goal: implement safe read-only thin-pool metadata interpretation.

Status (2026-07-10): initial read-only mapping implemented; hardening remains.

Tasks:
- parse thin metadata superblock, devices, mappings, and transaction ids;
- map virtual thin device blocks to data LV blocks;
- validate mapping with synthetic fixtures;
- fail closed for inconsistent transaction ids or missing metadata/data LVs.

Expected result:
- thin volumes can be identified and mapped only when metadata is complete and
  internally consistent.

### Stage 4 - Cluster Semantic Execution

Goal: enable controlled semantic execution after Stages 1-3 are verified.

Tasks:
- add semantic reconstruction commands;
- add UI analysis flow for multi-source evidence sets;
- add resumable per-node and per-volume jobs;
- add audit records for every source and reconstructed logical view.

Expected result:
- PVE cluster parsing becomes an explicit workflow with provenance and recovery.

## Test Matrix

| Area | Current milestone | Future cluster milestone |
| --- | --- | --- |
| Single E01 + MBR/GPT | Required | Required |
| LVM direct linear LV | Required | Required |
| XFS file tree | Required | Required |
| File preview | Required | Required |
| Linux artifacts | Required | Required |
| Multiple E01 sources | Import modeling only | Required |
| PVE host filesystem | LVM `pve/root` -> 64-bit EXT4 verified | Cross-node correlation |
| PVE config database | `/var/lib/pve-cluster/config.db` import and preview verified | Structured config correlation |
| LVM thin pool | Initial read-only dm-thin mapping; no repair/checksum claim | Hardening required after Stage 3 |
| Ceph BlueStore OSD | Label + BlueFS replay + RocksDB control-plane/SST/WAL + digest-only latest-state summary; no fake filesystem candidate | BlueStore semantic decode、RADOS/PG/object reconstruction required |
| LVM RAID/cache/snapshot | Unsupported diagnostic | Future optional |
| Partial/degraded VG | Unsupported diagnostic | Future optional |

## Acceptance Criteria

Current milestone:
- `检材3` root LV file tree enumerates successfully.
- Key files such as `/etc/passwd`, `/etc/os-release`, `/etc/fstab`,
  `/root/.bash_history`, and `/var/log/wtmp` can be previewed.
- Linux artifact extraction produces structured results from the real sample.
- `cluster_service::parse_cluster` returns `Unsupported` for semantic cluster
  reconstruction.
- Linux cluster import modeling can register and bounded-parallel import member
  images; each member remains source-isolated and the parent coordinator keeps
  aggregate state updates ordered.
- The private six-member PVE gate attempts every member in deterministic member
  order through the bounded scheduler,
  keeps `app.db` free of file-tree rows, verifies unique source DB paths, and
  records the expected host-ready/BlueStore-metadata aggregate outcome.

### BlueStore metadata boundary

The `disk02` metadata-only result is intentionally classified at the
media-format boundary:

1. The E01 container opens successfully, so this is not an E01 reader failure.
2. The outer device is an LVM PV. LVM expansion opens each readable LV and
   checks Ceph's `bluestore block device` label at LV-relative offsets `0`,
   `1 GiB`, `10 GiB`, `100 GiB`, and `1000 GiB`.
3. In the real `disk02` members, the label is at OSD-LV offset `0`; on
   `server01-disk02` that maps to image offset `1 MiB`.
4. The former defect was layer placement: only POSIX-filesystem LVs became
   candidates, while the BlueStore LV was discarded as an unknown filesystem.
5. BlueStore is a Ceph OSD block device, not a mountable POSIX filesystem.
6. Meow_Detective natively decodes the complete bdev label envelope, CRC32C,
   UUID, utime, metadata map, multi-label epoch, and replica positions.
7. For `bluefs=true`, import reads exactly 4 KiB at LV offset `4096`, validates
   the BlueFS envelope and independent CRC32C, binds the BlueFS OSD UUID to the
   selected label, and validates every log extent against the shared device.
8. The sanitized OSD inventory, BlueFS replay, RocksDB control-plane,
   `35/40/33` live-SST inventory, active-WAL metadata, and digest-only
   latest-state summaries commit in one source-database replacement
   transaction. Import remains `ready_metadata`, writes zero ordinary file
   entries, and does not run ordinary Linux artifact analysis.
9. Raw RocksDB key/value persistence, BlueStore onode/blob semantic decode,
   RADOS placement groups, objects, and VM disk reconstruction remain
   unsupported. The current RocksDB recovery boundary is specified in
   `docs/ceph-bluestore-stage6-design.md`.

The signature and offsets follow Ceph's
`src/ceph-volume/ceph_volume/util/disk.py` and Ceph's BlueStore implementation;
label structure and inspection behavior are defined by
`src/os/bluestore/bluestore_types.h` and
`src/os/bluestore/bluestore_tool.cc`. BlueStore must not be added to
`ImageFilesystemKind` until a separate object-store analysis architecture
exists.

Future cluster milestone:
- cluster evidence sets are explicit and auditable;
- every source has provenance;
- missing nodes/PVs are reported before execution;
- thin pool reconstruction is covered by synthetic and real fixtures;

### Stage 1 verified OSD inventory

The private six-member fixture was revalidated on 2026-07-13 through both the
native decoder and WSL Ceph Reef `ceph-bluestore-tool show-label`:

| Member | OSD | OSD UUID | Selected epoch |
|---|---:|---|---:|
| `server01-disk02` | 0 | `9630c2a5-650a-4395-a47a-ec496515bd61` | 23 |
| `server02-disk02` | 1 | `de8554de-f932-448d-be2c-0474df6c16c5` | 21 |
| `server03-disk02` | 2 | `cd6f9b5c-37d5-4dc0-8588-9669d156b02c` | 22 |

All three labels report cluster FSID
`3f28d8bb-e754-475b-b471-b9c97161bbf7`, RocksDB, BlueFS enabled, and Ceph
19.2.3 Squid creation metadata. `osd_key` is never persisted or logged; only
the boolean `osd_key_present` is retained.
- no guessed block mapping is accepted.

### Stage 2 verified BlueFS inventory

The same six-member fixture was rerun through the production desktop cluster
runner on 2026-07-13. All three OSDs expose CRC-valid version-2 BlueFS
superblocks with sequence `50`, block size `4096`, one shared-device log extent,
and no dedicated DB/WAL device:

| Member | BlueFS UUID | OSD UUID | Sequence | Shared bdev |
|---|---|---|---:|---:|
| `server01-disk02` | `394d12df-4023-44dc-b4c5-10b5e5dd48f4` | `9630c2a5-650a-4395-a47a-ec496515bd61` | 50 | 1 |
| `server02-disk02` | `e1b8a63e-3c93-4743-8232-b236b82fec83` | `de8554de-f932-448d-be2c-0474df6c16c5` | 50 | 1 |
| `server03-disk02` | `d8f0162e-aefe-4397-ad64-16b28af988a1` | `cd6f9b5c-37d5-4dc0-8588-9669d156b02c` | 50 | 1 |

The installed Ceph Reef `ceph-bluestore-tool` does not expose
`bluefs-super-dump`; the read-only WSL oracle therefore exports only the second
4 KiB device block and the native decoder validates it. This is a tool-version
limitation, not evidence of a missing superblock.

## Evaluation

Current milestone evaluation:
- run `cargo test -p app-services --test linux_e01_integration ... --ignored`
  for LVM expansion, XFS enumeration, preview, and Linux artifact extraction;
- run `cargo test -p fs-lvm`;
- run `cargo test -p fs-xfs`;
- run app-services compile checks.
- set `FORENSICS_PVE_CLUSTER_ROOT` and run
  `scripts/check-pve-cluster-import.ps1 -RequireFixture` for the complete
  desktop cluster lifecycle.

Future cluster evaluation:
- synthetic multi-PV fixtures;
- synthetic thin-pool metadata fixtures;
- real PVE host samples under `E:\pangushi\服务器`;
- cross-node artifact consistency checks;
- memory and runtime budgets per source and per LV.
