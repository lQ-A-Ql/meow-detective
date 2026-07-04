# PVE / Linux Cluster Parsing Design

## Summary

This document defines the reserved engineering boundary for future PVE / Linux
cluster parsing. The current milestone remains limited to single-disk Linux
server parsing: E01/Raw -> partition table -> LVM direct linear/striped LV ->
XFS -> file tree, preview, and Linux artifact extraction.

Cluster parsing is not enabled in this milestone. The codebase exposes only a
typed non-executing service boundary in `crates/app-services/src/cluster_service.rs`
so future PVE work has a clear module entry and does not leak into the
single-disk datasource or XFS parser path.

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
- Provide a typed cluster parsing service boundary that returns planned or
  unsupported status.
- Document the future PVE cluster architecture and test matrix.

### Out of Scope Now

- PVE cluster execution.
- Multi-node correlation of `/etc/pve` state.
- LVM thin-pool block mapping.
- LVM cache, RAID, snapshot, VDO, writecache reconstruction.
- Partial/degraded VG activation.
- Cross-E01 cluster import workflow or UI.
- Any frontend cluster controls.

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
  - currently returns `Unsupported`.

No Tauri command is registered for this interface in the current milestone.
That is deliberate: it is a backend design seam, not a user-facing capability.

## Stage Design

### Stage 0 - Current Milestone: XFS Single Disk

Goal: keep `检材3` working end to end.

Tasks:
- validate LVM direct LV expansion;
- validate XFS root tree enumeration;
- validate arbitrary file preview;
- validate Linux artifact extraction;
- keep cluster service non-executing.

Expected result:
- single-disk Linux server evidence is usable for investigation;
- cluster work remains impossible to trigger accidentally.

### Stage 1 - Cluster Evidence Set Modeling

Goal: model multi-source evidence without parsing thin pools.

Tasks:
- create case-level evidence set entities;
- group related E01/Raw sources by investigator selection;
- persist source roles such as `node`, `shared-storage`, `unknown`;
- expose plan-only diagnostics for missing members.

Expected result:
- the app can describe a PVE cluster evidence set and report readiness without
  attempting reconstruction.

### Stage 2 - PVE Host Filesystem Coverage

Goal: parse each node host OS independently.

Tasks:
- run the existing single-disk pipeline per source;
- extract `/etc/pve`, `/etc/corosync`, `/var/log/pve*`, systemd, auth, shell,
  and package artifacts;
- normalize node identity from hostname, machine-id, corosync config, and PVE
  config.

Expected result:
- investigators can compare host-level config and logs across nodes even before
  VM disk reconstruction is supported.

### Stage 3 - LVM Thin Metadata Research Spike

Goal: implement safe read-only thin-pool metadata interpretation.

Tasks:
- parse thin metadata superblock, devices, mappings, and transaction ids;
- map virtual thin device blocks to data LV blocks;
- validate mapping with synthetic fixtures;
- fail closed for inconsistent transaction ids or missing metadata/data LVs.

Expected result:
- thin volumes can be identified and mapped only when metadata is complete and
  internally consistent.

### Stage 4 - Cluster Import Execution

Goal: enable controlled execution after Stages 1-3 are verified.

Tasks:
- add Tauri command DTOs;
- add UI import flow for multi-source evidence sets;
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
| Multiple E01 sources | Interface only | Required |
| PVE `/etc/pve` extraction | As ordinary Linux artifacts | Cross-node correlation |
| LVM thin pool | Unsupported diagnostic | Required after Stage 3 |
| LVM RAID/cache/snapshot | Unsupported diagnostic | Future optional |
| Partial/degraded VG | Unsupported diagnostic | Future optional |

## Acceptance Criteria

Current milestone:
- `检材3` root LV file tree enumerates successfully.
- Key files such as `/etc/passwd`, `/etc/os-release`, `/etc/fstab`,
  `/root/.bash_history`, and `/var/log/wtmp` can be previewed.
- Linux artifact extraction produces structured results from the real sample.
- `cluster_service::parse_cluster` returns `Unsupported`.
- No frontend or command path exposes cluster execution.

Future cluster milestone:
- cluster evidence sets are explicit and auditable;
- every source has provenance;
- missing nodes/PVs are reported before execution;
- thin pool reconstruction is covered by synthetic and real fixtures;
- no guessed block mapping is accepted.

## Evaluation

Current milestone evaluation:
- run `cargo test -p app-services --test linux_e01_integration ... --ignored`
  for LVM expansion, XFS enumeration, preview, and Linux artifact extraction;
- run `cargo test -p fs-lvm`;
- run `cargo test -p fs-xfs`;
- run app-services compile checks.

Future cluster evaluation:
- synthetic multi-PV fixtures;
- synthetic thin-pool metadata fixtures;
- real PVE host samples under `E:\pangushi\服务器`;
- cross-node artifact consistency checks;
- memory and runtime budgets per source and per LV.
