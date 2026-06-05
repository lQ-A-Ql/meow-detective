# Issues

## 2026-06-04 Wave 1 Exploration
- No small CI-safe mountable RAW/E01 fixture was found for image-backed full import tests. Avoid making non-ignored full image import characterization depend on external `FORENSICS_E01_FIXTURE`.
- MCP contract decision is required before broad MCP implementation: either normalize to camelCase or explicitly test snake_case exceptions.

## 2026-06-04 Task 4 Frontend Large-Data Baselines
- `typescript-language-server` is not installed in this environment, so `lsp_diagnostics` could not validate the changed frontend test files; targeted Vitest execution was used as the available verification path.

## 2026-06-04 Task 3 MCP Contract Baseline
- MCP remains internally inconsistent by design for this task: top-level Tauri args use camelCase but MCP DTO/request object fields use snake_case. Task 13/14 should decide whether to normalize DTOs to camelCase or preserve explicit snake_case exceptions.
- LSP diagnostics were unavailable in this environment: rust-analyzer.exe is missing from the stable Rust toolchain and typescript-language-server is not installed.

## 2026-06-04 Task 2 Provenance Contract Baseline
- Future provenance fields are intentionally not present yet: `cargo test -p transport future_provenance_contract_includes_hash_version_and_confidence -- --ignored` fails at missing `sourceHash`. Keep this blocked on the planned schema migration tasks rather than adding fields early.

## 2026-06-04 Task 5 Quality Gate Baseline
- No missing required frontend quality gate scripts found in `frontend/package.json`.

## 2026-06-04 Task 7 Artifact and Timeline Provenance Fields
- LSP diagnostics remained unavailable because `rust-analyzer.exe` is missing from the stable Windows toolchain; targeted cargo tests, affected crate compile, and `cargo fmt --all -- --check` were used as verification.

## 2026-06-04 Task 8 Report Provenance Propagation
- `cargo test -p transport reports artifacts timeline` is not valid Cargo syntax for multiple test filters; use `cargo test -p transport` for DTO contract coverage, or run separate filtered commands.
- LSP diagnostics remained unavailable because `rust-analyzer.exe` is missing from the stable Windows toolchain; cargo check/tests/fmt were used instead.

## 2026-06-05 Task 10 Contract Synchronization Tests
- LSP diagnostics remain unavailable: Rust reports `Unknown binary 'rust-analyzer.exe' in official toolchain 'stable-x86_64-pc-windows-msvc'`, and frontend reports missing `typescript-language-server`. Verification used `cargo test -p transport`, focused Vitest, `pnpm typecheck`, and `cargo fmt --all -- --check`.

## 2026-06-05 Task 11 Import Seam Extraction
- Blocked by repeated subagent access failures: the preferred Task 11 session returned no evidence file/no effective seam, then retries failed with `unknown certificate verification error`; a general-agent retry failed with `Invalid API Key`. Per continuation directive, Task 11 was marked `- [~]` in the plan rather than silently skipped.

## 2026-06-05 Task 12 Import Worker/Staging Boundary Extraction
- Marked `- [~]` because it is explicitly blocked by Task 11 in the plan. Task 11 could not be completed due repeated subagent access/credential failures, so Task 12 cannot be safely executed without the required preceding import seam.
- Follow-up session completed Task 12 after Task 11 was verified complete. LSP diagnostics are still unavailable because `rust-analyzer.exe` is missing from the stable Windows toolchain; cargo tests/check/fmt were used as the verification path.

## 2026-06-05 Task 13 MCP API Layer Normalization
- LSP diagnostics are still unavailable for changed TypeScript files because `typescript-language-server` is not installed; focused Vitest and `pnpm --dir frontend typecheck` passed and were recorded in `.omo/evidence/task-13-mcp-api-layer.txt`.

## 2026-06-05 Task 14 MCP DTO Casing and Error Compatibility
- LSP diagnostics remain unavailable in this environment for changed Rust/TypeScript files (`rust-analyzer.exe` missing from stable toolchain; `typescript-language-server` not installed). Verification used cargo/Vitest/typecheck gates instead.

## 2026-06-05 Task 15 DenseDataTable Virtualization Fix
- `typescript-language-server` is still unavailable, so `lsp_diagnostics` could not validate the changed table files. Verification used the required focused Vitest command plus `pnpm --dir frontend typecheck`, both passing; exact outputs were saved to `.omo/evidence/task-15-dense-table-virtualization.txt`.

## 2026-06-05 Task 16 Viewer Large-Content Guardrails
- `typescript-language-server` is still unavailable in this environment, so `lsp_diagnostics` could not validate the changed viewer files. Verification used the required focused Vitest command and `pnpm --dir frontend typecheck`; exact outputs were saved to `.omo/evidence/task-16-viewer-guardrails.txt`.

## 2026-06-05 Task 17 Timeline Query Index and Pagination Safeguards
- LSP diagnostics remain unavailable for changed Rust files because `rust-analyzer.exe` is missing from the stable Windows toolchain; targeted cargo tests, `cargo check -p persistence-sqlite`, migration runner coverage, and `cargo fmt --all -- --check` were used instead.

## 2026-06-05 Task 18 Search Highlight and Indexing Memory Safeguards
- LSP diagnostics remain unavailable for changed Rust files because `rust-analyzer.exe` is missing from the stable Windows toolchain; `cargo test -p search`, app-services search tests, `cargo check -p search`, and `cargo fmt --all -- --check` were used instead.

## 2026-06-05 Task 19 Full Workspace Quality Gate Execution
- LSP diagnostics remain unavailable for changed Rust/TypeScript files (`rust-analyzer.exe` missing from stable Windows toolchain; `typescript-language-server` not installed). Full Rust and frontend command gates passed after fixes and were used as verification.
- Workspace `cargo test --workspace` still includes ignored/manual E01 and real fixture tests unless `FORENSICS_E01_FIXTURE` or local fixtures are provided; this is documented in Task 19 evidence and did not fail the default gate.

## 2026-06-05 Task 20 Regression Matrix Synthesis
- Final remediation reporting must keep fixture-gated E01 and real-sample scenarios labeled as manual or follow-up coverage, not as default automated regression coverage.
- Final remediation reporting must keep the LSP gap explicit because command gates replaced that verification path throughout the wave.
