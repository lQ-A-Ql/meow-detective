# Performance UX Optimization Plan

## TL;DR
> **Summary**: Turn the already-started performance and UX design into an executable, dependency-ordered ticket plan that preserves the current typed import progress bridge and cancellation work, then finishes the remaining backend, frontend, and verification tasks.
> **Deliverables**: tracked implementation tickets, dependency matrix, verification wave, and success criteria aligned to the current working tree.
> **Effort**: M
> **Parallel**: Limited, after backend contract and import-state work
> **Critical Path**: typed import contract and cancellation baseline -> partial result freshness -> scheduling and cache visibility -> instrumentation -> frontend UX -> evidence hash/report caveats -> final quality summary

## Context
### Original Request
User asked to execute the performance and UX optimization work after the detailed design was already produced, but no saved plan file existed yet.

### Current Baseline
- Current learnings and the active diff confirm the first three tickets are already in flight or completed in the worktree.
- The started scope covers transport DTOs and event topics, typed import phase progress bridging, and stronger cancellation state handling without reopening the whole import design.
- This file is a tracking artifact only. It does not authorize extra code changes beyond the defined tickets.

### Confirmed Started Scope
- Ticket 1 is confirmed by the learnings note for Performance UX Ticket 1 and the current diff in `crates/transport/src/dto/`, `crates/transport/src/events/mod.rs`, `frontend/src/types/models.ts`, and `frontend/src/lib/events/tauri-bridge.ts`.
- Ticket 2 is confirmed by the learnings note for Performance UX Ticket 2 and the current diff in `apps/desktop/src-tauri/src/commands/import/pipeline.rs` and `apps/desktop/src-tauri/src/events/event_bridge.rs`.
- Ticket 3 is confirmed by the learnings note for Performance UX Ticket 3 and the current diff in `apps/desktop/src-tauri/src/commands/import/pipeline.rs` and `crates/persistence-sqlite/src/repositories/job_repo.rs`.

### Guardrails
- Preserve the legacy `job-progress` bridge while the typed events are adopted.
- Keep using the exact event topics `import.phase_progress`, `import.partial_result`, `job.cancellation`, `cache.index_status`, and `performance.report_ready`.
- Keep private local evidence samples out of plan content and future evidence summaries.
- Treat LSP diagnostics as unavailable in this environment. Use targeted command evidence later when implementation resumes.

## Work Objectives
### Core Objective
Finish the performance and UX design as a sequence of executable tickets that improve import transparency, partial-result freshness, scheduling visibility, cache and index reuse clarity, and investigator-facing responsiveness without widening scope into unrelated refactors.

### Deliverables
- A dependency-ordered ticket list with only the already-confirmed first three tickets checked.
- A clear backend-to-frontend execution path for typed progress, partial results, cache status, and report-ready performance signals.
- A final verification wave that captures command evidence and UX checks after implementation is complete.

### Must Have
- Tickets 1 through 3 remain marked complete because they are supported by notes and the current diff.
- Future tickets stay unchecked until their code and verification land.
- Verification remains bounded and avoids broad expensive test runs at planning time.

### Must NOT Have
- No code changes from this plan task.
- No reopening of `.omo/plans/engineering-audit-remediation.md`.
- No sample-specific private paths in the plan.

## Verification Strategy
- Planning-time verification is limited to current notes plus `git status --short` and `git diff --stat`.
- Implementation-time verification should favor targeted Rust and frontend commands over broad workspace gates until the final wave.
- Environment caveat stays explicit: LSP diagnostics are unavailable here, so later implementation tasks should record cargo and pnpm evidence instead.
- This planning task intentionally does not run expensive tests.

## Execution Strategy
### Execution Order
1. Lock the typed contract and import-state baseline as the foundation.
2. Expose backend partial-result freshness and cache status before frontend UX wiring.
3. Add scheduling and instrumentation so the UI can explain delays and progress honestly.
4. Finish the existing layout-based frontend UX and evidence-hash/report caveat surfacing.
5. Close with default quality gates and a compact performance evidence summary.

### Parallelism Notes
- Tickets 4 and 5 can start after Ticket 3, but Ticket 4 should land before Ticket 8 because the frontend needs the backend freshness contract.
- Ticket 6 can proceed after Ticket 4 if cache reuse status depends on partial-result metadata, otherwise it can run beside Ticket 5.
- Ticket 7 should finish before Ticket 10 so the final summary includes actual benchmark and instrumentation outputs.

## Dependency Matrix
| Ticket | Depends On | Blocks |
|---|---|---|
| 1 | none | 2, 4, 8 |
| 2 | 1 | 4, 5, 8 |
| 3 | 1, 2 | 4, 5, 8 |
| 4 | 1, 2, 3 | 6, 8, 9, 10 |
| 5 | 2, 3 | 7, 8, 10 |
| 6 | 4 | 8, 10 |
| 7 | 5 | 10 |
| 8 | 4, 5, 6 | 10 |
| 9 | 4 | 10 |
| 10 | 4, 5, 6, 7, 8, 9 | Final Verification Wave |

## TODOs
- [x] 1. Define import progress and cancellation DTO and event contract.
  Scope: lock the frontend-facing DTOs, enums, TypeScript parity, and event-topic constants for typed progress and cancellation. Preserve compatibility with the legacy bridge while standardizing the new topic names.
  Evidence today: learnings confirm camelCase DTOs, lowerCamelCase enum wire values, and the new topics `import.phase_progress`, `import.partial_result`, `job.cancellation`, `cache.index_status`, and `performance.report_ready`.

- [x] 2. Emit typed import phase progress from the Tauri import pipeline.
  Scope: map existing import profile phases into typed phase events while keeping legacy `job-progress` behavior intact.
  Evidence today: learnings confirm typed phase mapping from attach, probe, enumerate, merge, analyze, and finalize phases, with conservative metric parsing until richer instrumentation exists.

- [x] 3. Harden import cancellation state transitions and job persistence.
  Scope: make request, acknowledgement, draining, and terminal cancellation states explicit in emitted events and persisted job status details.
  Evidence today: learnings confirm the `job.cancellation` flow, `safeToClose=true` on terminal cancellation, and `jobs.status` use of `cancelling` and `cancelled` without a new migration.

- [x] 4. Expose partial results and freshness visibility from backend services.
  Scope: emit and persist bounded partial-result metadata through `import.partial_result` so the UI can distinguish ready, stale, deferred, and still-processing analysis slices.
  Exit signal: backend services and transport types expose enough status for the UI to label partial evidence honestly.

- [x] 5. Implement artifact scheduling and worker budget strategy.
  Scope: make worker-budget and stage-scheduling decisions observable enough to explain why artifact extraction or analysis is queued, running slowly, or intentionally deferred.
  Exit signal: typed progress payloads or related service outputs communicate bounded scheduling state rather than opaque waiting.

- [x] 6. Surface cache and index reuse plus invalidation status.
  Scope: emit `cache.index_status` updates that tell the UI whether timeline, search, or artifact caches are reused, warming, stale, or invalidated.
  Exit signal: investigators can see whether the system is reusing prior work or rebuilding it.

- [x] 7. Add timeline and search performance instrumentation and benchmarks.
  Scope: collect bounded timing and throughput evidence for timeline queries, search indexing, and related hot paths, then prepare `performance.report_ready` outputs for the final summary.
  Evidence today: learnings confirm bounded `PerformanceReportDto` metrics for timeline query, search query, and search indexing hot paths, plus focused Rust tests and desktop command compilation evidence.

- [x] 8. Wire frontend import progress, cancellation, and partial-result UX into existing layout components.
  Scope: use the current layout system, not a redesign, to show typed phases, cancellation state, partial-result freshness, cache reuse, and report readiness in the existing investigator flow.
  Exit signal: the frontend reflects backend truth using the typed event topics and existing layout components.

- [x] 9. Add evidence hash background status and report caveat visibility.
  Scope: surface background hash progress and explicit caveats when evidence hashes or derived report sections are pending, unavailable, or intentionally deferred.
  Exit signal: reports and UI status stop implying completeness before evidence hashing or dependent work actually finishes.

- [x] 10. Close with default quality gates and a performance evidence summary.
  Scope: run the final targeted and default command gates that make sense for the touched areas, then summarize typed event coverage, UX behavior, benchmarks, and known caveats.
  Exit signal: performance and UX work finishes with command evidence, not just code inspection.

## Final Verification Wave
- [x] Verify transport and frontend event-topic parity for `import.phase_progress`, `import.partial_result`, `job.cancellation`, `cache.index_status`, and `performance.report_ready`.
- [x] Run focused Rust verification for the import pipeline, cancellation persistence, and any new backend partial-result or cache-status services touched by Tickets 4 through 7.
- [x] Run focused frontend verification for typed progress, cancel state, partial-result freshness, cache-status UX, and report-ready messaging in the existing layout.
- [x] Run the smallest sensible quality gates for touched Rust crates and frontend modules, then record the explicit LSP-unavailable caveat.
- [x] Produce a compact performance evidence summary that calls out benchmark numbers, remaining fixture limits, and any intentional deferred states.

## Success Criteria
- The plan matches the current working tree, with only Tickets 1 through 3 marked complete.
- Remaining tickets are unchecked and ordered by real dependencies.
- Backend and frontend work stay aligned around the typed event contract without dropping the legacy progress bridge too early.
- Final verification expectations are visible before implementation resumes.
- This plan remains a scoped execution tracker, not a broad redesign brief.
