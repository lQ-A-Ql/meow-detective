# Stage 6 Frontend Runtime Audit

**Date**: 2026-07-05
**Baseline**: `2f2cccd chore: lock stage5 engineering regressions`
**Scope**: `frontend/src` runtime code, excluding tests, test setup, and fixture-only files.

## Summary

Stage 6 adds a frontend-specific engineering pass for high coupling and mock/demo residue. The audit found no runtime API mock fallback and no direct Tauri `invoke` calls outside the API client. Production frontend demo-case creation entry points were removed, and selected page/component API calls were moved behind feature hooks.

## Runtime Boundary Findings

| Area | Result | Evidence |
|---|---|---|
| Tauri invoke boundary | Pass | Only `frontend/src/lib/api/client.ts` imports `@tauri-apps/api/core` and calls `invoke`. |
| Runtime API mocks | Pass | `vi.mock`, `mockResolvedValue`, and API test doubles are confined to `*.test.ts(x)` files. |
| Demo/fake business datasets | Pass | No non-test runtime module defines mock/fake/dummy forensic datasets. |
| Demo command usage | Removed from production frontend | `DataAnalysis`, `V2Workbench`, and shared analysis empty/header components no longer expose `create_analysis_demo_case`. |
| No-op demo action | Fixed | `V3Dashboard` now renders a case-required empty state without a no-op demo callback. |
| Page/component API coupling | Improved | Reports, Settings, CaseHome, Marketplace, and Notebook citation lookup now route through feature hooks instead of direct business API imports. |
| Notebook citation search | Fixed | Citation search no longer derives neighborhood start ids from `nodeCountByType`; it lists real graph nodes by default and only expands neighborhoods from explicit selected graph node ids. |
| Local storage | Allowed | Used for UI preferences/settings mirror and saved graph/search queries, not backend result simulation. |
| `Math.random` | Allowed with guard | Used for graph layout jitter, skeleton width, and local saved query ID fallback only. |
| `setTimeout` | Allowed with guard | Used for UI copy/menu/search debounce behavior only. |

## Guard Coverage

`scripts/check-frontend-runtime-guard.ps1` now enforces:

- `frontend/src/lib/api/client.ts` remains the single Tauri `invoke` entry point.
- Runtime files outside tests do not contain test/mock API wiring.
- Runtime files do not define mock/fake/dummy business data collections.
- Runtime files do not expose mock/demo runtime modes or production demo-case creation entry points.
- Pages/components/stores do not import business API modules directly; feature hooks own business API access.
- Graph citation/search code cannot use node type count keys as graph node ids.
- `setTimeout` and `Math.random` remain limited to explicitly allowed UI/layout/local-ID use cases.

## Cleanup Completed

- `frontend/src/components/analysis/panels/SystemInfoPanel.tsx`
  - Removed demo-case props and buttons from `AnalysisHeader` and `AnalysisEmptyState`.
- `frontend/src/app/pages/V3Dashboard.tsx`
  - Removed the no-op `onLoadDemoCase={() => {}}` callback.
  - Empty state now communicates that a case is required without presenting a dead demo action.
- `frontend/src/app/pages/DataAnalysis.tsx` and `frontend/src/app/pages/V2Workbench.tsx`
  - Removed production demo-case creation hooks and buttons.
- `frontend/src/features/case/hooks.ts`, `frontend/src/lib/api/case.ts`, and `frontend/src/lib/api/commands.ts`
  - Removed frontend runtime exports for the demo-case command.
- `frontend/src/features/settings/hooks.ts`, `frontend/src/features/reports/hooks.ts`, and `frontend/src/features/rule-packs/hooks.ts`
  - Centralized settings, report export, and rule-pack API access behind feature hooks.
- `frontend/src/features/graph/hooks.ts` and `frontend/src/components/notebook/NotebookEntryForm.tsx`
  - Moved citation node lookup into graph hooks, stopped querying graph neighborhoods with node type strings, and added a real `list_graph_nodes` path for default citation candidates.

## Remaining Engineering Debt

| Priority | Item | Boundary |
|---|---|---|
| P2 | Several page-level components still coordinate many hooks and view states. | Future refactors should split page orchestration from presentational sections without changing data contracts. |
| P2 | Settings still mirrors operational values into local storage after backend save. | Current behavior preserves existing UX; a later settings hardening pass should separate operational backend settings from local UI drafts. |
| P2 | Graph layout uses random initial jitter, which is acceptable for visualization but not deterministic for screenshots. | If visual regression testing is introduced, inject a deterministic layout seed. |
| P2 | Saved query IDs fall back to `Math.random` when `crypto.randomUUID` is unavailable. | Acceptable local-only state; can be replaced with a monotonic fallback if needed. |

## Acceptance Status

- No frontend mock fallback remains in runtime code.
- Production frontend no longer exposes demo-case creation entry points.
- Frontend direct invoke boundary is guarded.
- Page/component business API coupling is guarded through feature hook boundaries.
- Stage 6 does not change Hex preview behavior, XFS parser behavior, PVE cluster scope, or backend evidence parsing.
