# Batch Processing Design

## Overview

The batch processing pipeline allows an investigator to queue a multi-phase job
(`Mount`, `Catalog`, `ExtractArtifacts`, `Index`, `Correlate`, `Export`) against
one or more data sources and then monitor its progress.

## Current Implementation Scope

As of V3, the following command surface is implemented:

- `create_batch_plan` — builds a persisted `BatchPlan` and returns a queued job.
- `get_batch_job` — returns the current status and phase breakdown.
- `list_batch_jobs` — lists all jobs for the active case.

The execution control commands are **MVP stubs** and will return an
`Unsupported` error if called:

- `start_batch`
- `pause_batch`
- `resume_batch`
- `cancel_batch`

These stubs exist so the UI and API contract can be wired end-to-end while the
V3 scheduler (async task spawning, checkpointing, cancellation) is still under
development.
