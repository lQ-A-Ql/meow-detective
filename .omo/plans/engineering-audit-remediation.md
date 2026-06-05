# Engineering Audit Remediation Plan

## TL;DR
> **Summary**: Remediate the audit findings through TDD-first waves covering contract safety, forensic provenance, import pipeline decoupling, frontend large-data performance, and final quality gates.
> **Deliverables**: executable remediation tasks; synchronized Rust/TypeScript DTO updates; provenance schema and report improvements; import service seams; MCP contract normalization; frontend virtualization/viewer fixes; automated verification evidence.
> **Effort**: XL
> **Parallel**: YES - 5 waves
> **Critical Path**: Contract/provenance tests → provenance schema/DTO → import service seams → performance fixes → full quality gates

## Context
### Original Request
用户要求：制定改进方案。整改依据为前序全量工程化审计，覆盖功能实现、架构、算法性能/时空复杂度和取证可信度。

### Interview Summary
- 整改优先级：全量分波。
- 测试策略：TDD。
- 当前阶段只生成计划，不修改源码。

### Metis Review (gaps addressed)
- Guardrail: do not rewrite the entire import pipeline in one task; split by observable seams and preserve behavior first.
- Guardrail: provenance model must not balloon into full legal chain-of-custody; implement bounded evidence/source/parser/confidence metadata.
- Guardrail: DTO changes must synchronize Rust DTOs, TypeScript models, API wrappers, mocks, commands, and tests.
- Guardrail: frontend performance remediation must target virtualization/materialization only, not broad UI redesign.
- Guardrail: all acceptance criteria are agent-executable; no manual QA-only completion.

## Work Objectives
### Core Objective
Convert the engineering audit findings into tested, bounded improvements that reduce architecture coupling, improve forensic trust, normalize API contracts, and prevent large-data UI/performance regressions.

### Deliverables
- TDD characterization tests for current import, provenance, MCP, report, and frontend large-data behavior.
- Bounded provenance fields across domain, SQLite migrations, transport DTOs, frontend types, mocks, and report/export surfaces.
- Import pipeline service seams that move orchestration from Tauri command layer into `app-services` without behavior loss.
- Normalized MCP API layer and TypeScript/Rust contract alignment.
- Virtualized/paginated frontend large-data rendering and viewer streaming/truncation safeguards.
- Final evidence under `.omo/evidence/` showing commands/tests run by the executor.

### Definition of Done (verifiable conditions with commands)
- `cargo fmt --all -- --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes.
- From `frontend/`: `pnpm typecheck`, `pnpm lint`, `pnpm test`, and `pnpm build` pass.
- New/updated tests prove DTO/provenance/MCP/import/performance behavior.
- No frontend mock forensic output can be displayed without explicit mock-mode labeling.

### Must Have
- TDD first for each task: failing test/contract check before implementation.
- Backwards-compatible migrations or explicit compatibility tests.
- Rust and TypeScript contract synchronization.
- Evidence files for each task in `.omo/evidence/`.

### Must NOT Have
- No one-shot rewrite of `apps/desktop/src-tauri/src/commands/import/pipeline.rs`.
- No broad UI redesign.
- No full legal chain-of-custody/policy engine scope.
- No DTO shape changes without frontend models, mocks, and tests.
- No task marked complete through manual inspection only.

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: TDD + existing Rust workspace tests + frontend Vitest/jsdom tests.
- QA policy: Every implementation task includes happy path and failure/edge QA scenarios.
- Evidence: `.omo/evidence/task-{N}-{slug}.{ext}`.

## Execution Strategy
### Parallel Execution Waves
> Target: 5-8 tasks per wave. <3 per wave (except final) = under-splitting.
> Extract shared dependencies as Wave-1 tasks for max parallelism.

Wave 1: TDD characterization and contract baselines (Tasks 1-5)
Wave 2: Provenance model/schema/DTO/report/mock labeling (Tasks 6-10)
Wave 3: Import pipeline service seams and MCP normalization (Tasks 11-14)
Wave 4: Frontend/search/timeline performance safeguards (Tasks 15-18)
Wave 5: Cross-cutting quality gates and regression consolidation (Tasks 19-20)

### Dependency Matrix (full, all tasks)
| Task | Depends On | Blocks |
|---|---|---|
| 1 | none | 6, 11 |
| 2 | none | 6, 7, 8, 9, 10 |
| 3 | none | 13, 14 |
| 4 | none | 15, 16, 17, 18 |
| 5 | none | 19, 20 |
| 6 | 2 | 7, 8, 9, 10 |
| 7 | 6 | 8, 9, 10 |
| 8 | 7 | 10, 19 |
| 9 | 7 | 10, 19 |
| 10 | 8, 9 | 19 |
| 11 | 1 | 12 |
| 12 | 11 | 19 |
| 13 | 3 | 14, 19 |
| 14 | 13 | 19 |
| 15 | 4 | 19 |
| 16 | 4 | 19 |
| 17 | 4 | 19 |
| 18 | 4 | 19 |
| 19 | 8, 9, 10, 12, 14, 15, 16, 17, 18 | 20 |
| 20 | 19 | Final Verification |

### Agent Dispatch Summary (wave → task count → categories)
| Wave | Count | Categories |
|---|---:|---|
| 1 | 5 | unspecified-high, quick, visual-engineering |
| 2 | 5 | deep, unspecified-high, writing |
| 3 | 4 | deep, unspecified-high |
| 4 | 4 | visual-engineering, unspecified-high, ultrabrain |
| 5 | 2 | unspecified-high |

## TODOs
> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

- [x] 1. Import Pipeline Characterization Tests

  **What to do**: Add failing-first characterization tests around current import behavior before refactoring. Cover request validation, logical directory import, image-backed source mode behavior, job status, and event emission where existing test seams allow. Use `apps/desktop/src-tauri/src/commands/import/pipeline.rs`, `crates/app-services/src/file_service/enumeration.rs`, and existing command/service test patterns.
  **Must NOT do**: Do not move import logic in this task. Do not alter behavior to make tests pass except minimal test harness setup.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: cross-crate Rust/Tauri characterization.
  - Skills: [] - Existing Rust test patterns should suffice.
  - Omitted: [`dfir-*`] - No external forensic evidence analysis is needed.

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 11 | Blocked By: none

  **References**:
  - Pattern: `apps/desktop/src-tauri/src/commands/import/pipeline.rs` - current behavior to preserve.
  - Pattern: `crates/app-services/src/file_service/enumeration.rs` - file enumeration and batch insertion behavior.
  - Test: existing Rust workspace tests under `crates/app-services/src` and `apps/desktop/src-tauri/src` - follow local test style.

  **Acceptance Criteria**:
  - [ ] A test fails before any import refactor if current job/import semantics are broken.
  - [ ] Test evidence saved to `.omo/evidence/task-1-import-characterization.txt` with `cargo test` command and result.
  - [ ] No production import orchestration moved in this task.

  **QA Scenarios**:
  ```
  Scenario: Logical directory import baseline
    Tool: Bash
    Steps: Run targeted Rust tests for the new logical-directory import characterization test.
    Expected: Test passes after baseline harness captures current behavior.
    Evidence: .omo/evidence/task-1-import-characterization.txt

  Scenario: Image-backed source disables timeline projection baseline
    Tool: Bash
    Steps: Run targeted Rust test asserting image-backed import config preserves current timeline projection behavior.
    Expected: Test passes and documents current image-backed behavior.
    Evidence: .omo/evidence/task-1-import-characterization-image.txt
  ```

  **Commit**: YES | Message: `test(import): characterize pipeline behavior` | Files: Rust test files only

- [x] 2. Provenance Contract Baseline Tests

  **What to do**: Add failing-first tests that document current provenance gaps and expected future fields for DataSource, Artifact, TimelineEvent, Analysis provenance, and Report DTOs. Tests should initially assert desired new fields are present after implementation, not silently accept gaps.
  **Must NOT do**: Do not add schema fields in this task unless needed to make a compile-time contract test scaffold possible.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: Rust DTO/domain/persistence/frontend contract baseline.
  - Skills: [] - Contract tests only.
  - Omitted: [`security-review`] - This is trust/provenance modeling, not a full vulnerability audit.

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 6, 7, 8, 9, 10 | Blocked By: none

  **References**:
  - API/Type: `crates/domain/src/datasource/mod.rs` - current source metadata.
  - API/Type: `crates/domain/src/artifact/mod.rs` - artifact provenance gap.
  - API/Type: `crates/domain/src/timeline/mod.rs` - timeline provenance gap.
  - API/Type: `crates/transport/src/dto/analysis.rs` - existing parser/path/status/warnings provenance.
  - API/Type: `crates/transport/src/dto/artifacts.rs`, `crates/transport/src/dto/timeline.rs`, `crates/transport/src/dto/reports.rs` - DTOs to extend later.

  **Acceptance Criteria**:
  - [ ] Tests or compile-time assertions define required fields: evidence/source hash, parser/extractor id, parser/extractor version, confidence, source attribution.
  - [ ] Test evidence saved to `.omo/evidence/task-2-provenance-contract.txt`.
  - [ ] Desired scope remains bounded; no legal chain-of-custody/policy engine fields are required.

  **QA Scenarios**:
  ```
  Scenario: DTO desired field contract
    Tool: Bash
    Steps: Run targeted tests for transport DTO serialization expectations.
    Expected: Tests fail before implementation or pass after fields are added; output records field coverage.
    Evidence: .omo/evidence/task-2-provenance-contract.txt

  Scenario: Bounded scope enforcement
    Tool: Bash
    Steps: Run tests or static assertions verifying no unrelated policy/notarization fields are required.
    Expected: Contract only includes bounded provenance fields.
    Evidence: .omo/evidence/task-2-provenance-scope.txt
  ```

  **Commit**: YES | Message: `test(provenance): define trust contract` | Files: Rust/TypeScript test files

- [x] 3. MCP Contract Baseline Tests

  **What to do**: Add TDD tests proving MCP command arguments and responses are consistently normalized across `frontend/src/stores/mcp-store.ts`, a new or existing frontend API layer, `crates/transport/src/dto/mcp.rs`, and `apps/desktop/src-tauri/src/commands/mcp_commands.rs`.
  **Must NOT do**: Do not redesign MCP transport/security/sandbox behavior.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: frontend/Rust contract consistency.
  - Skills: [] - Contract and unit tests.
  - Omitted: [`oh-my-opencode`] - This MCP is project runtime functionality, not OpenCode configuration.

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 13, 14 | Blocked By: none

  **References**:
  - Pattern: `frontend/src/stores/mcp-store.ts` - current direct API calls and local interfaces.
  - API/Type: `crates/transport/src/dto/mcp.rs` - Rust MCP DTOs.
  - Pattern: `apps/desktop/src-tauri/src/commands/mcp_commands.rs` - command names and argument shapes.
  - Pattern: `frontend/src/lib/api/client.ts` - central request mechanism.

  **Acceptance Criteria**:
  - [ ] Frontend tests fail if MCP store bypasses normalized API layer for changed calls.
  - [ ] Serialization tests or documented snapshots cover command arg casing.
  - [ ] Evidence saved to `.omo/evidence/task-3-mcp-contract.txt`.

  **QA Scenarios**:
  ```
  Scenario: MCP command arg casing
    Tool: Bash
    Steps: Run frontend unit tests for MCP API request payloads.
    Expected: Payload keys match backend command expectations.
    Evidence: .omo/evidence/task-3-mcp-contract.txt

  Scenario: Malformed MCP response
    Tool: Bash
    Steps: Run store/API test with missing/unknown MCP response fields.
    Expected: Test asserts safe parse/fallback behavior without direct store bypass.
    Evidence: .omo/evidence/task-3-mcp-malformed.txt
  ```

  **Commit**: YES | Message: `test(mcp): capture api contract` | Files: frontend/Rust tests

- [x] 4. Frontend Large-Data Baseline Tests

  **What to do**: Add frontend tests or performance-oriented assertions for `DenseDataTable`, `VirtualFileTree`, `HexViewer`, and `TextViewer` showing desired behavior with large inputs: bounded DOM nodes, virtual rendering, truncation/streaming indicators, and no full DOM materialization.
  **Must NOT do**: Do not restyle or redesign UI.

  **Recommended Agent Profile**:
  - Category: `visual-engineering` - Reason: frontend behavior and UI component testing.
  - Skills: [] - Vitest/jsdom tests should be enough.
  - Omitted: [`frontend-design`] - No visual redesign requested.

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 15, 16, 17, 18 | Blocked By: none

  **References**:
  - Pattern: `frontend/src/components/tables/DenseDataTable.tsx` - currently risks all-row mapping.
  - Pattern: `frontend/src/components/tree/VirtualFileTree.tsx` - virtual tree pattern.
  - Pattern: `frontend/src/components/viewers/HexViewer.tsx` - hex parsing/materialization.
  - Pattern: `frontend/src/components/viewers/TextViewer.tsx` - full-content split risk.
  - Test: frontend Vitest tests under `frontend/src`.

  **Acceptance Criteria**:
  - [ ] Tests define 10k-row/table or large-content expectations.
  - [ ] Tests check DOM node count or rendered slice, not visual appearance only.
  - [ ] Evidence saved to `.omo/evidence/task-4-frontend-large-data.txt`.

  **QA Scenarios**:
  ```
  Scenario: Large table bounded render
    Tool: Bash
    Steps: Run Vitest for DenseDataTable with 10k synthetic rows.
    Expected: Rendered DOM rows remain bounded to visible/windowed rows.
    Evidence: .omo/evidence/task-4-large-table.txt

  Scenario: Large viewer truncation/streaming guard
    Tool: Bash
    Steps: Run Vitest for HexViewer/TextViewer with large content.
    Expected: Component shows bounded render/truncation/streaming state without materializing all lines into DOM.
    Evidence: .omo/evidence/task-4-large-viewer.txt
  ```

  **Commit**: YES | Message: `test(frontend): capture large data limits` | Files: frontend tests

- [x] 5. Quality Gate Baseline Script Documentation

  **What to do**: Add or update a plan-local evidence checklist for exact commands executors must run. If repository already has scripts, reference them; otherwise do not create code scripts unless necessary. Capture command baselines from README/AGENTS.
  **Must NOT do**: Do not run all quality gates in this task unless implementation changes already require it.

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: checklist and baseline confirmation.
  - Skills: [] - Documentation/evidence checklist only.
  - Omitted: [`gh-fix-ci`] - No CI failure is being debugged.

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 19, 20 | Blocked By: none

  **References**:
  - Pattern: `README.md` - quality gate commands.
  - Pattern: `AGENTS.md` - project commands and conventions.
  - Pattern: `frontend/package.json` - frontend scripts.
  - Pattern: root `Cargo.toml` - workspace context.

  **Acceptance Criteria**:
  - [ ] Evidence checklist includes Rust and frontend commands exactly.
  - [ ] Checklist states working directory for frontend commands is `frontend/`.
  - [ ] Evidence saved to `.omo/evidence/task-5-quality-baseline.md`.

  **QA Scenarios**:
  ```
  Scenario: Command checklist completeness
    Tool: Bash
    Steps: Verify command list against README/AGENTS/package scripts without executing expensive gates.
    Expected: All required commands are present with working directories.
    Evidence: .omo/evidence/task-5-quality-baseline.md

  Scenario: Missing command detection
    Tool: Bash
    Steps: Check `frontend/package.json` contains referenced scripts.
    Expected: Missing scripts are listed as blockers, not assumed.
    Evidence: .omo/evidence/task-5-script-check.txt
  ```

  **Commit**: YES | Message: `docs(qa): record remediation gate checklist` | Files: `.omo/evidence/*` only if committed by executor policy, or no commit if evidence excluded

- [x] 6. DataSource Provenance Schema and Domain Fields

  **What to do**: Implement bounded evidence source provenance fields in domain and SQLite schema: source hash or hash status, canonical source path where applicable, evidence size, reader kind, provenance status/warnings. Add migrations and repository tests proving round-trip behavior.
  **Must NOT do**: Do not implement full legal chain-of-custody, signing, notarization, or policy engine.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: schema/domain/repository migration impact.
  - Skills: [] - Rust/database tests.
  - Omitted: [`dfir-mulder`] - No external evidence analysis.

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: 7, 8, 9, 10 | Blocked By: 2

  **References**:
  - API/Type: `crates/domain/src/datasource/mod.rs` - add bounded fields.
  - Pattern: `crates/persistence-sqlite/src/migrations/scripts/0002_data_sources.sql` - existing schema.
  - Pattern: `crates/persistence-sqlite/src/repositories` - repository round-trip patterns.
  - Pattern: `crates/evidence-core/src/reader/mod.rs` - reader metadata source.

  **Acceptance Criteria**:
  - [ ] New migration adds bounded provenance columns with nullable/backward-compatible defaults.
  - [ ] Repository tests prove old rows and new rows load correctly.
  - [ ] `cargo test -p persistence-sqlite` or targeted equivalent passes.
  - [ ] Evidence saved to `.omo/evidence/task-6-datasource-provenance.txt`.

  **QA Scenarios**:
  ```
  Scenario: New DataSource provenance round trip
    Tool: Bash
    Steps: Run targeted persistence tests inserting a data source with hash/status/reader metadata.
    Expected: Loaded domain object preserves all bounded provenance fields.
    Evidence: .omo/evidence/task-6-datasource-provenance.txt

  Scenario: Legacy data source compatibility
    Tool: Bash
    Steps: Run migration/repository test against a row lacking new provenance fields.
    Expected: Row loads with safe `unknown`/None defaults, no panic.
    Evidence: .omo/evidence/task-6-legacy-compat.txt
  ```

  **Commit**: YES | Message: `feat(provenance): add data source metadata` | Files: domain, migrations, repositories, tests

- [x] 7. Artifact and Timeline Provenance Fields

  **What to do**: Extend Artifact and TimelineEvent domain/persistence/DTO paths with parser/extractor id, parser/extractor version, confidence, and source attribution. Preserve `source_object_id` and make new fields optional/backward-compatible.
  **Must NOT do**: Do not require every parser to provide high confidence immediately; unknown/partial must be valid explicit states.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: multi-crate schema/DTO propagation.
  - Skills: [] - Rust contract and repository tests.
  - Omitted: [`security-review`] - Not a vulnerability audit.

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: 8, 9, 10 | Blocked By: 6

  **References**:
  - API/Type: `crates/domain/src/artifact/mod.rs` - artifact fields.
  - API/Type: `crates/domain/src/timeline/mod.rs` - timeline fields.
  - Pattern: `crates/persistence-sqlite/src/migrations/scripts/0004_artifacts.sql` - artifact schema.
  - Pattern: `crates/persistence-sqlite/src/migrations/scripts/0005_timeline_events.sql` - timeline schema.
  - API/Type: `crates/transport/src/dto/artifacts.rs`, `crates/transport/src/dto/timeline.rs` - DTO propagation.

  **Acceptance Criteria**:
  - [ ] Artifact and timeline repository round trips preserve parser/extractor/confidence fields.
  - [ ] DTO serialization uses camelCase and optional fields omit when None where existing convention applies.
  - [ ] Existing callers compile with explicit default mapping.
  - [ ] Evidence saved to `.omo/evidence/task-7-artifact-timeline-provenance.txt`.

  **QA Scenarios**:
  ```
  Scenario: Artifact provenance round trip
    Tool: Bash
    Steps: Run targeted artifact repository and DTO serialization tests.
    Expected: Parser/extractor id/version/confidence/source attribution survive DB and DTO layers.
    Evidence: .omo/evidence/task-7-artifact-provenance.txt

  Scenario: Timeline unknown confidence
    Tool: Bash
    Steps: Run timeline test with missing parser confidence.
    Expected: DTO exposes explicit unknown/None without failure.
    Evidence: .omo/evidence/task-7-timeline-unknown-confidence.txt
  ```

  **Commit**: YES | Message: `feat(provenance): tag artifacts and timeline` | Files: domain, transport, migrations, repositories, tests

- [x] 8. Analysis and Report Provenance Propagation

  **What to do**: Update analysis service/report service/exporters to consume and emit new provenance metadata in summaries, artifact exports, timeline/report rows, and generated report sections. Preserve existing report output while adding source/parser/confidence columns or sections.
  **Must NOT do**: Do not make reports immutable evidence bundles or notarized packages.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: service/exporter DTO propagation.
  - Skills: [] - Rust tests and snapshot assertions.
  - Omitted: [`pdf`] - No PDF output requested.

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: 10, 19 | Blocked By: 7

  **References**:
  - Pattern: `crates/app-services/src/analysis_service.rs` - current parser/path/status/warnings provenance.
  - Pattern: `crates/app-services/src/report_service.rs` - report summary/export orchestration.
  - Pattern: `crates/reports/src/html/exporter.rs`, `crates/reports/src/json/exporter.rs` - exporters.
  - API/Type: `crates/transport/src/dto/reports.rs` - report DTOs.

  **Acceptance Criteria**:
  - [ ] Report/export tests assert provenance appears for artifact/timeline/report rows.
  - [ ] Analysis DTOs preserve warnings/status plus new confidence/source semantics.
  - [ ] Existing report generation still works for incomplete provenance.
  - [ ] Evidence saved to `.omo/evidence/task-8-report-provenance.txt`.

  **QA Scenarios**:
  ```
  Scenario: Report includes provenance
    Tool: Bash
    Steps: Run report service/exporter tests with artifacts containing parser/confidence metadata.
    Expected: HTML/CSV/JSON output includes provenance fields or sections.
    Evidence: .omo/evidence/task-8-report-provenance.txt

  Scenario: Incomplete provenance report
    Tool: Bash
    Steps: Run report test with legacy artifact/timeline rows lacking new metadata.
    Expected: Report marks provenance as unknown/partial, does not fail.
    Evidence: .omo/evidence/task-8-report-legacy.txt
  ```

  **Commit**: YES | Message: `feat(reports): surface provenance metadata` | Files: app-services, reports, transport, tests

- [x] 9. Frontend Types, Mock Data, and Mock-Mode Labeling

  **What to do**: Synchronize frontend `types/models.ts`, API mocks, and UI display with provenance fields. Add explicit mock-mode labeling/watermark/banner for forensic-looking mock data, especially mock WannaCry/YARA/registry/timeline content.
  **Must NOT do**: Do not remove mock mode; do not redesign app shell.

  **Recommended Agent Profile**:
  - Category: `visual-engineering` - Reason: frontend types and visible trust labeling.
  - Skills: [] - React/Vitest tests.
  - Omitted: [`frontend-design`] - Only bounded labeling, not redesign.

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: 10, 19 | Blocked By: 7

  **References**:
  - API/Type: `frontend/src/types/models.ts` - frontend DTO mirror.
  - Pattern: `frontend/src/lib/api/mock-data.ts` - forensic-looking mock content.
  - Pattern: `frontend/src/lib/api/client.ts` - mock vs Tauri mode.
  - Pattern: layout components in `frontend/src/components/layout/` - preferred app-shell location per AGENTS.

  **Acceptance Criteria**:
  - [ ] Frontend types include new optional provenance fields.
  - [ ] Mock data includes explicit mock/source provenance fields.
  - [ ] UI test proves mock mode is visibly labeled when mock data is shown.
  - [ ] Evidence saved to `.omo/evidence/task-9-frontend-provenance-mock.txt`.

  **QA Scenarios**:
  ```
  Scenario: Mock mode visible label
    Tool: Bash
    Steps: Run frontend test rendering app/API mock mode.
    Expected: A visible label indicates mock/demo data, not real forensic output.
    Evidence: .omo/evidence/task-9-mock-label.txt

  Scenario: Provenance fields render safely
    Tool: Bash
    Steps: Run component/API tests with artifact/timeline provenance fields and with fields missing.
    Expected: UI renders known metadata and safe unknown labels without crashing.
    Evidence: .omo/evidence/task-9-provenance-ui.txt
  ```

  **Commit**: YES | Message: `feat(frontend): label mock forensic data` | Files: frontend types, mock data, layout/UI tests

- [x] 10. Rust-TypeScript Contract Synchronization Tests

  **What to do**: Add serialization snapshots or contract tests that compare Rust DTO casing/field presence with TypeScript models and mock data for changed DataSource, Artifact, Timeline, Analysis, Report, and MCP DTOs.
  **Must NOT do**: Do not introduce full codegen unless explicitly scoped and justified by tests.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: cross-language contract verification.
  - Skills: [] - Snapshot/contract tests.
  - Omitted: [`codemap`] - No broad mapping needed.

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: 19 | Blocked By: 8, 9

  **References**:
  - API/Type: `crates/transport/src/dto/*.rs` - Rust DTO source.
  - API/Type: `frontend/src/types/models.ts` - TypeScript mirror.
  - Pattern: `frontend/src/lib/api/mock-data.ts` - mock contract data.
  - Pattern: `crates/transport/src/commands/mod.rs` - command request DTOs.

  **Acceptance Criteria**:
  - [ ] Contract tests fail on missing changed fields or casing mismatch.
  - [ ] MCP exception handling is normalized or explicitly tested.
  - [ ] Evidence saved to `.omo/evidence/task-10-contract-sync.txt`.

  **QA Scenarios**:
  ```
  Scenario: DTO casing snapshot
    Tool: Bash
    Steps: Run Rust serialization snapshot tests and frontend model/mock tests.
    Expected: Changed fields use intended camelCase/snake_case contract consistently.
    Evidence: .omo/evidence/task-10-dto-snapshot.txt

  Scenario: Mock contract parity
    Tool: Bash
    Steps: Run frontend tests validating mock data against TypeScript interfaces for changed surfaces.
    Expected: Mock data satisfies real DTO shape including provenance fields.
    Evidence: .omo/evidence/task-10-mock-parity.txt
  ```

  **Commit**: YES | Message: `test(contract): sync rust and typescript dto shapes` | Files: Rust/frontend contract tests

- [x] 11. Import Orchestration Service Seam Extraction

  **What to do**: Extract the first bounded seam from `apps/desktop/src-tauri/src/commands/import/pipeline.rs` into `crates/app-services`: request classification/source validation/import configuration construction. Keep Tauri command behavior identical and covered by Task 1 tests.
  **Must NOT do**: Do not move staging merge, event emission, and worker orchestration in the same task.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: architecture refactor with behavior preservation.
  - Skills: [] - Rust refactor and tests.
  - Omitted: [`simplify`] - This is architectural seam extraction, not generic simplification.

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: 12 | Blocked By: 1

  **References**:
  - Pattern: `apps/desktop/src-tauri/src/commands/import/pipeline.rs` - current orchestration.
  - Pattern: `crates/app-services/src/import_precheck.rs`, `crates/app-services/src/import_state.rs` - adjacent import service modules.
  - Pattern: `crates/app-services/src/lib.rs` - service module exports.

  **Acceptance Criteria**:
  - [ ] New app-services seam has unit tests for source validation/classification/config.
  - [ ] Existing import characterization tests still pass.
  - [ ] Tauri command delegates seam instead of duplicating logic.
  - [ ] Evidence saved to `.omo/evidence/task-11-import-seam.txt`.

  **QA Scenarios**:
  ```
  Scenario: Classification seam preserves behavior
    Tool: Bash
    Steps: Run targeted tests for new import service seam and existing import characterization.
    Expected: Same logical/image mode decisions as baseline.
    Evidence: .omo/evidence/task-11-import-seam.txt

  Scenario: Invalid source path handling
    Tool: Bash
    Steps: Run test with missing/not-file/not-dir path.
    Expected: Service returns same error semantics as current command path.
    Evidence: .omo/evidence/task-11-invalid-source.txt
  ```

  **Commit**: YES | Message: `refactor(import): extract source classification seam` | Files: app-services, src-tauri command, tests

- [x] 12. Import Worker/Staging Boundary Extraction

  **What to do**: Extract a second bounded seam for post-import analysis worker/staging orchestration into `app-services`, preserving Tauri progress/event emission through callbacks or explicit event adapter. Keep command layer as validation/state/event wrapper.
  **Must NOT do**: Do not alter indexing/timeline/artifact semantics beyond adapter wiring.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: high-risk refactor across import analysis and Tauri events.
  - Skills: [] - Rust architecture/testing.
  - Omitted: [`dfir-*`] - No external artifact analysis.

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: 19 | Blocked By: 11

  **References**:
  - Pattern: `apps/desktop/src-tauri/src/commands/import/pipeline.rs` - existing event/progress orchestration.
  - Pattern: `crates/app-services/src/import_analysis.rs` - post-import worker/staging logic.
  - Pattern: `crates/app-services/src/staging.rs` - staging DB ownership.
  - Pattern: `crates/transport/src/events/mod.rs` - event topic contract.

  **Acceptance Criteria**:
  - [ ] Service-level tests cover worker/staging orchestration with fake event/progress sink.
  - [ ] Tauri command no longer owns core worker/staging orchestration.
  - [ ] Existing import characterization tests still pass.
  - [ ] Evidence saved to `.omo/evidence/task-12-import-worker-boundary.txt`.

  **QA Scenarios**:
  ```
  Scenario: Progress adapter receives events
    Tool: Bash
    Steps: Run service test with fake progress/event sink.
    Expected: Expected progress/job events are emitted through adapter.
    Evidence: .omo/evidence/task-12-progress-adapter.txt

  Scenario: Failed worker preserves job status
    Tool: Bash
    Steps: Run service test simulating worker failure.
    Expected: Job status and warnings/errors match baseline semantics.
    Evidence: .omo/evidence/task-12-worker-failure.txt
  ```

  **Commit**: YES | Message: `refactor(import): move worker orchestration to services` | Files: app-services, src-tauri command, tests

- [x] 13. MCP API Layer Normalization

  **What to do**: Create or complete `frontend/src/lib/api/mcp.ts` as the only frontend MCP request layer. Move MCP request construction from `frontend/src/stores/mcp-store.ts` into API functions with typed responses and normalized argument casing.
  **Must NOT do**: Do not change MCP server protocol behavior.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: frontend API/store refactor.
  - Skills: [] - TypeScript tests.
  - Omitted: [`oh-my-opencode`] - Not OpenCode MCP config.

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: 14, 19 | Blocked By: 3

  **References**:
  - Pattern: `frontend/src/stores/mcp-store.ts` - current direct calls.
  - Pattern: `frontend/src/lib/api/client.ts` - API request abstraction.
  - Pattern: `frontend/src/lib/api/*.ts` - feature API style.
  - Pattern: `apps/desktop/src-tauri/src/commands/mcp_commands.rs` - backend command names.

  **Acceptance Criteria**:
  - [ ] MCP store delegates all backend calls to `frontend/src/lib/api/mcp.ts`.
  - [ ] Frontend tests prove normalized payload shapes.
  - [ ] TypeScript typecheck passes for MCP store/API.
  - [ ] Evidence saved to `.omo/evidence/task-13-mcp-api-layer.txt`.

  **QA Scenarios**:
  ```
  Scenario: Store delegates to MCP API
    Tool: Bash
    Steps: Run frontend unit test mocking `mcp.ts` API methods.
    Expected: Store calls API methods and does not build raw command payloads.
    Evidence: .omo/evidence/task-13-store-delegation.txt

  Scenario: Command payload normalization
    Tool: Bash
    Steps: Run tests for each changed MCP API method payload.
    Expected: Payload keys match backend expectations.
    Evidence: .omo/evidence/task-13-payloads.txt
  ```

  **Commit**: YES | Message: `refactor(mcp): centralize frontend api calls` | Files: frontend API/store/tests

- [x] 14. MCP DTO Casing and Error Compatibility

  **What to do**: Normalize MCP DTO casing strategy in Rust/TypeScript. Either add `serde(rename_all = "camelCase")` where appropriate or explicitly map snake_case in the API layer. Add malformed response/version mismatch tests.
  **Must NOT do**: Do not leave mixed conventions undocumented and untested.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: Rust/TS DTO compatibility.
  - Skills: [] - Contract tests.
  - Omitted: [`security-review`] - Not MCP security hardening.

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: 19 | Blocked By: 13

  **References**:
  - API/Type: `crates/transport/src/dto/mcp.rs` - Rust DTO casing.
  - Pattern: `apps/desktop/src-tauri/src/commands/mcp_commands.rs` - command serialization boundary.
  - API/Type: `frontend/src/types/models.ts` - TypeScript model location.
  - Pattern: `frontend/src/lib/api/mcp.ts` - normalized API layer from Task 13.

  **Acceptance Criteria**:
  - [ ] MCP DTO casing is documented by tests.
  - [ ] Unknown/malformed MCP responses are handled safely.
  - [ ] Contract tests from Task 3 pass.
  - [ ] Evidence saved to `.omo/evidence/task-14-mcp-dto-casing.txt`.

  **QA Scenarios**:
  ```
  Scenario: MCP casing compatibility
    Tool: Bash
    Steps: Run Rust serialization tests and frontend MCP API tests.
    Expected: Field names match the chosen casing strategy end-to-end.
    Evidence: .omo/evidence/task-14-casing.txt

  Scenario: MCP version mismatch
    Tool: Bash
    Steps: Run frontend test with response containing missing/unknown versioned fields.
    Expected: UI/store reports safe error or fallback without crash.
    Evidence: .omo/evidence/task-14-version-mismatch.txt
  ```

  **Commit**: YES | Message: `fix(mcp): normalize dto contract` | Files: transport, frontend types/API/tests

- [x] 15. DenseDataTable Virtualization Fix

  **What to do**: Update `DenseDataTable` so large tables do not render every row into DOM. Use existing project patterns or minimal virtualization/windowing. Preserve column sorting/filter behavior if present.
  **Must NOT do**: Do not redesign table appearance or change feature semantics.

  **Recommended Agent Profile**:
  - Category: `visual-engineering` - Reason: frontend component performance.
  - Skills: [] - React/Vitest.
  - Omitted: [`frontend-design`] - No visual redesign.

  **Parallelization**: Can Parallel: YES | Wave 4 | Blocks: 19 | Blocked By: 4

  **References**:
  - Pattern: `frontend/src/components/tables/DenseDataTable.tsx` - current table implementation.
  - Pattern: `frontend/src/components/tree/VirtualFileTree.tsx` - existing virtual rendering pattern.
  - Test: frontend component tests under `frontend/src`.

  **Acceptance Criteria**:
  - [ ] Large-table test from Task 4 passes with bounded DOM rows.
  - [ ] Existing table interactions still pass.
  - [ ] `pnpm test` targeted for table tests passes.
  - [ ] Evidence saved to `.omo/evidence/task-15-dense-table-virtualization.txt`.

  **QA Scenarios**:
  ```
  Scenario: 10k row table render
    Tool: Bash
    Steps: Run DenseDataTable test with 10k rows.
    Expected: DOM rows are bounded and visible rows render correctly.
    Evidence: .omo/evidence/task-15-10k-table.txt

  Scenario: Sort/filter preserved
    Tool: Bash
    Steps: Run table interaction tests after virtualization.
    Expected: Sorting/filtering still returns expected visible rows.
    Evidence: .omo/evidence/task-15-table-interactions.txt
  ```

  **Commit**: YES | Message: `perf(frontend): virtualize dense data table` | Files: DenseDataTable, tests

- [x] 16. Hex/Text Viewer Large Content Guardrails

  **What to do**: Update `HexViewer` and `TextViewer` to avoid unsafe full-content DOM rendering and expose truncation/streaming/large-file states. If content must be split in memory due current API, ensure DOM remains bounded and user sees clear limits.
  **Must NOT do**: Do not implement a new backend streaming protocol unless existing API cannot satisfy tests; if needed, document as blocker before expanding scope.

  **Recommended Agent Profile**:
  - Category: `visual-engineering` - Reason: viewer UX/performance behavior.
  - Skills: [] - React/Vitest.
  - Omitted: [`playwright`] - Unit/component tests should verify core behavior.

  **Parallelization**: Can Parallel: YES | Wave 4 | Blocks: 19 | Blocked By: 4

  **References**:
  - Pattern: `frontend/src/components/viewers/HexViewer.tsx` - hex viewer materialization risk.
  - Pattern: `frontend/src/components/viewers/TextViewer.tsx` - text split risk.
  - API: `frontend/src/lib/api/files.ts` - content/range read APIs.
  - Backend: `apps/desktop/src-tauri/src/commands/file_commands.rs` - file range/content commands.

  **Acceptance Criteria**:
  - [ ] Large viewer tests pass with bounded DOM rendering.
  - [ ] UI exposes truncation/large-content state when not all content is shown.
  - [ ] Existing small-file viewer tests still pass.
  - [ ] Evidence saved to `.omo/evidence/task-16-viewer-guardrails.txt`.

  **QA Scenarios**:
  ```
  Scenario: Large text content bounded render
    Tool: Bash
    Steps: Run TextViewer test with large synthetic content.
    Expected: DOM renders bounded lines and shows large-content/truncated indicator.
    Evidence: .omo/evidence/task-16-text-large.txt

  Scenario: Large hex content bounded render
    Tool: Bash
    Steps: Run HexViewer test with large synthetic buffer/string.
    Expected: DOM rows are bounded and offsets remain correct for visible range.
    Evidence: .omo/evidence/task-16-hex-large.txt
  ```

  **Commit**: YES | Message: `perf(viewers): bound large content rendering` | Files: HexViewer, TextViewer, tests

- [x] 17. Timeline Query Index and Pagination Safeguards

  **What to do**: Verify and improve timeline repository query indexes/pagination for large event sets. Add tests for unfiltered ordering, filtered case/type/time queries, identical timestamps, and missing timestamps.
  **Must NOT do**: Do not change timeline event semantics except deterministic ordering/pagination.

  **Recommended Agent Profile**:
  - Category: `ultrabrain` - Reason: query/index correctness and performance.
  - Skills: [] - SQLite/Rust tests.
  - Omitted: [`dfir-plaso-timeline`] - This is app timeline code, not external Plaso.

  **Parallelization**: Can Parallel: YES | Wave 4 | Blocks: 19 | Blocked By: 4

  **References**:
  - Pattern: `crates/app-services/src/timeline_service.rs` - projection/query service.
  - Pattern: `crates/persistence-sqlite/src/repositories/timeline_repo.rs` - query implementation.
  - Pattern: `crates/persistence-sqlite/src/migrations/scripts/0005_timeline_events.sql` - timeline schema/indexes.

  **Acceptance Criteria**:
  - [ ] Repository tests cover large ordered timeline query and filters.
  - [ ] Migration/index update added if tests show missing index.
  - [ ] Pagination returns deterministic ordering for identical timestamps.
  - [ ] Evidence saved to `.omo/evidence/task-17-timeline-query.txt`.

  **QA Scenarios**:
  ```
  Scenario: Large unfiltered timeline query
    Tool: Bash
    Steps: Run repository test with many events and `ORDER BY ts DESC` path.
    Expected: Query uses deterministic pagination and passes within test expectations.
    Evidence: .omo/evidence/task-17-large-timeline.txt

  Scenario: Identical timestamp ordering
    Tool: Bash
    Steps: Run repository test with multiple events sharing timestamp.
    Expected: Stable secondary ordering prevents duplicate/missing pagination entries.
    Evidence: .omo/evidence/task-17-identical-timestamps.txt
  ```

  **Commit**: YES | Message: `perf(timeline): harden query pagination` | Files: timeline repo/migrations/tests

- [x] 18. Search Highlight and Indexing Memory Safeguards

  **What to do**: Add tests and bounded behavior for search query highlighting/index writing so large stored content does not create avoidable memory pressure. Prefer snippet limits and content caps consistent with existing API.
  **Must NOT do**: Do not replace Tantivy or redesign search indexing.

  **Recommended Agent Profile**:
  - Category: `ultrabrain` - Reason: algorithm/performance guardrails.
  - Skills: [] - Rust tests.
  - Omitted: [`deep`] - Scope is bounded to search code.

  **Parallelization**: Can Parallel: YES | Wave 4 | Blocks: 19 | Blocked By: 4

  **References**:
  - Pattern: `crates/search/src/indexer/tantivy_writer.rs` - index writer/query path.
  - Pattern: `crates/search/src/highlighter/mod.rs` - content lowercasing/highlighting.
  - Pattern: `crates/app-services/src/search_service.rs` - app-facing search behavior.

  **Acceptance Criteria**:
  - [ ] Tests cover large content highlighting with bounded snippets.
  - [ ] Query output remains correct for zero/many results.
  - [ ] Existing search tests pass.
  - [ ] Evidence saved to `.omo/evidence/task-18-search-memory.txt`.

  **QA Scenarios**:
  ```
  Scenario: Large content highlighting
    Tool: Bash
    Steps: Run search highlighter test with large synthetic content and limited snippets.
    Expected: Returned snippets are bounded and correct.
    Evidence: .omo/evidence/task-18-large-highlight.txt

  Scenario: Stale or missing index
    Tool: Bash
    Steps: Run search service test for missing/stale index condition.
    Expected: Service returns safe empty/error state matching existing contract.
    Evidence: .omo/evidence/task-18-missing-index.txt
  ```

  **Commit**: YES | Message: `perf(search): bound highlighting memory use` | Files: search/app-services tests and code

- [x] 19. Full Workspace Quality Gate Execution

  **What to do**: Run all required quality gates after implementation tasks complete. Fix failures by routing back to the owning task area, then rerun until green.
  **Must NOT do**: Do not skip clippy warnings, do not use `--no-verify`, do not mark complete with known failing tests.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: cross-workspace verification and failure triage.
  - Skills: [] - Command execution and targeted fixes.
  - Omitted: [`gh-fix-ci`] - Local gates, not GitHub Actions failure.

  **Parallelization**: Can Parallel: NO | Wave 5 | Blocks: 20 | Blocked By: 8, 9, 10, 12, 14, 15, 16, 17, 18

  **References**:
  - Commands: `README.md` and `AGENTS.md` - quality gate list.
  - Frontend scripts: `frontend/package.json` - pnpm script names.
  - Rust workspace: root `Cargo.toml`.

  **Acceptance Criteria**:
  - [ ] `cargo fmt --all -- --check` passes.
  - [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
  - [ ] `cargo test --workspace` passes.
  - [ ] From `frontend/`: `pnpm typecheck`, `pnpm lint`, `pnpm test`, `pnpm build` pass.
  - [ ] Evidence saved to `.omo/evidence/task-19-quality-gates.txt`.

  **QA Scenarios**:
  ```
  Scenario: Backend quality gates
    Tool: Bash
    Steps: Run cargo fmt check, clippy, and workspace tests.
    Expected: All pass with no skipped warnings.
    Evidence: .omo/evidence/task-19-backend-gates.txt

  Scenario: Frontend quality gates
    Tool: Bash
    Steps: From frontend/, run pnpm typecheck, lint, test, build.
    Expected: All pass.
    Evidence: .omo/evidence/task-19-frontend-gates.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: none unless fixes are needed; fixes should commit under owning scope

- [x] 20. Remediation Evidence and Regression Matrix

  **What to do**: Create a final regression matrix tying each original audit risk to implemented tasks, tests, commands, and evidence files. Include unresolved risks explicitly as follow-up, not hidden assumptions.
  **Must NOT do**: Do not claim remediation for risks not verified by tests/evidence.

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: evidence-backed technical summary.
  - Skills: [] - Documentation synthesis.
  - Omitted: [`Humanizer`] - Precision over prose polish.

  **Parallelization**: Can Parallel: NO | Wave 5 | Blocks: Final Verification | Blocked By: 19

  **References**:
  - Evidence: `.omo/evidence/task-*` - task outputs.
  - Plan: `.omo/plans/engineering-audit-remediation.md` - risk/task mapping.
  - Prior audit findings summarized in this plan Context.

  **Acceptance Criteria**:
  - [ ] Matrix maps import coupling, provenance gaps, MCP drift, frontend performance, timeline/search performance, mock trust, and quality gates to concrete evidence.
  - [ ] Unresolved risks are listed with owner task/follow-up recommendation.
  - [ ] Evidence saved to `.omo/evidence/task-20-regression-matrix.md`.

  **QA Scenarios**:
  ```
  Scenario: Risk-to-evidence completeness
    Tool: Bash
    Steps: Verify each original audit risk has at least one evidence file reference or unresolved label.
    Expected: No risk is silently omitted.
    Evidence: .omo/evidence/task-20-regression-matrix.md

  Scenario: No unsupported remediation claims
    Tool: Bash
    Steps: Search final matrix for claims without task/evidence references.
    Expected: Unsupported claims are removed or marked unresolved.
    Evidence: .omo/evidence/task-20-unsupported-claims.txt
  ```

  **Commit**: YES | Message: `docs(audit): add remediation evidence matrix` | Files: evidence/report docs if tracked by executor policy

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [x] F1. Plan Compliance Audit — oracle
- [x] F2. Code Quality Review — unspecified-high
- [x] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [x] F4. Scope Fidelity Check — deep

## Commit Strategy
- Use atomic commits by remediation area, not one giant commit.
- Commit tests before implementation when practical:
  - `test(import): characterize pipeline behavior`
  - `test(provenance): define trust contract`
  - `feat(provenance): add data source metadata`
  - `feat(provenance): tag artifacts and timeline`
  - `refactor(import): extract source classification seam`
  - `refactor(import): move worker orchestration to services`
  - `fix(mcp): normalize dto contract`
  - `perf(frontend): virtualize dense data table`
  - `perf(viewers): bound large content rendering`
  - `perf(timeline): harden query pagination`
  - `perf(search): bound highlighting memory use`

## Success Criteria
- Import orchestration is moved behind tested app-services seams while preserving current behavior.
- Provenance metadata survives domain → persistence → service → transport → frontend/report paths.
- MCP calls use a normalized API layer and tested DTO casing/response handling.
- Large frontend tables/viewers avoid full DOM materialization and expose bounded rendering states.
- Timeline/search performance risks are covered by targeted tests and indexes/bounds where needed.
- Mock forensic-looking output is explicitly labeled as mock/demo data.
- Full backend and frontend quality gates pass with evidence.
