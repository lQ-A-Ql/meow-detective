# Frontend MVP Boundary

This document is the authoritative frontend engineering boundary for Meow~Detective.
It complements `AGENTS.md`, `docs/design-constraints.md`, and
`scripts/check-frontend-runtime-guard.ps1`.

## Layer Contract

| Layer | Path | Responsibility | Forbidden |
|---|---|---|---|
| Page shell | `frontend/src/app/pages/*.tsx` | Route entry that renders one feature container | React state/effects, UI composition, direct store, API, platform adapter, or Tauri imports |
| Feature model / container | `frontend/src/features/<domain>/hooks.ts`, `use-*-model.ts`, `containers/*` | TanStack Query, mutations, store/platform/event access, UI state orchestration, navigation actions | Backend business calculation and fake data |
| Domain component | `frontend/src/features/<domain>/components/*` | Reusable domain UI with explicit props | Host filesystem access and direct Tauri invoke |
| Shared component | `frontend/src/components/*` | Cross-domain layout, table, tree, viewer, status primitives | Domain state, business API imports, stores, platform adapters |
| API layer | `frontend/src/lib/api/*` | Typed Tauri command wrappers only | UI state, routing, presentation formatting |
| Platform layer | `frontend/src/lib/platform/*` | Tauri/browser plugin adapters such as dialogs | Business DTO transformation |
| Store layer | `frontend/src/stores/*` | Global UI state and selected IDs | Backend-derived facts or forensic conclusions |

## Component Ownership

- Components must not live under `frontend/src/app/pages` except route pages and tests.
- A migrated route page renders its feature container only. Query orchestration, selection state,
  persisted UI state, navigation, and event subscriptions belong in the feature model/container;
  JSX belongs in the feature component.
- Cross-domain UI primitives belong in `frontend/src/components/*`.
- Domain-specific components belong in `frontend/src/features/<domain>/components/*`, even when currently used by one route.
- Components under `features/<domain>/components/*` are pure views: they receive DTO-shaped data and callbacks only. They must not import API modules, stores, platform adapters, Tauri packages, or router/store navigation actions.
- Components may import feature model or hook types with `import type`, but must not execute feature hooks or page models. Runtime orchestration stays in `containers/*` and `use-*-model.ts`.
- Components that need API/store/platform/event access must be split so the container/model sits in `features/<domain>/containers/*` or `use-*-model.ts` and the component receives props/callbacks.
- Feature hooks and page models must not import runtime UI modules. A type-only presentation contract is tolerated when needed to describe an injected view state; rendering dependencies remain one-way from components to props.
- Layout shell components are the only shared component exception allowed to read `ui-store`.

## Shared UI Primitives

- `frontend/src/app/components/ui/button.tsx` is the single shared action-button primitive.
- Product action buttons and button-like runtime controls must import `Button`; business
  components must not hand-write raw `<button>` or maintain private button primitives.
- Allowed raw button exceptions are framework or compound-control internals only:
  `SidebarMenuButton`, Radix/select internals, and sidebar rail/menu internals guarded by
  `scripts/check-frontend-runtime-guard.ps1`.
- If a new repeated action style is needed, add a named `Button` variant/size first; do not create
  a second ad-hoc button component.
- Text, number, date, path, and search fields must use `Input`.
- Multi-line editing fields must use `Textarea`.
- Dropdowns must use `Select`, `SelectTrigger`, `SelectContent`, and `SelectItem`.
- Boolean toggles that are semantically checkboxes must use `Checkbox`.
- Repeated label + control + hint/error layout must use the shared `Field` family.
- Data row/column datasets must use `DenseDataTable`; the low-level `Table` primitive is only
  for `DenseDataTable` internals.
- Every runtime `DenseDataTable` must be placed in the shared `DenseDataTableFrame`. Embedded
  tables pass their real `rowCount` so sparse data does not reserve empty space; full-screen
  workspaces use `layout="fill"` only when their parent establishes a bounded flex height chain.
  Feature-local table viewport wrappers are forbidden.
- Page and panel tabs must use `Tabs`, `TabsList`, `TabsTrigger`, and `TabsContent`; do not
  emulate tabs with groups of `Button`.
- Repeated viewer tabs should use `ViewerTabFrame`; repeated analysis/detail tabs should use
  `PanelTabs`. These wrappers are semantic compositions around the single `Tabs` primitive,
  not second primitive sets.
- Repeated metric/stat displays must use `MetricCard` or `StatGrid`.
- Repeated key/value displays must use `KeyValueField`.
- Repeated section headers, panels, and empty states must use `SectionHeader`, `PanelFrame`, or
  `EmptyState`.

## Raw Control Exceptions

Raw controls are forbidden in production page/feature/shared components unless they are one of
the documented semantic exceptions below and are allowlisted by
`scripts/check-frontend-runtime-guard.ps1`.

- `input[type=range]` inside media viewers, because the range element is the native seek/volume
  interaction surface.
- Read-only Markdown task-list checkbox HTML generated by the notebook renderer.
- UI primitive implementation files themselves, such as `input.tsx`, `select.tsx`,
  `checkbox.tsx`, `textarea.tsx`, and `table.tsx`.
- Button primitive and sidebar compound implementation internals that render the actual
  semantic DOM button.

Any new exception must be documented here and added narrowly to the guard whitelist.

## Data Authority

- Backend DTOs are the source of truth for cases, evidence, file systems, artifacts, hash values, timelines, parser state, partitions, LVM/XFS semantics, and aggregate statistics.
- Frontend may format bytes, dates, labels, tabs, selection IDs, and input validation.
- Frontend must not recompute forensic facts, synthesize fallback datasets, or silently replace unavailable backend data with mock data.

## Runtime Boundaries

- `frontend/src/lib/api/client.ts` is the only `@tauri-apps/api/core` invoke entry point.
- `frontend/src/lib/events/tauri-bridge.ts` is the event adapter.
- `frontend/src/lib/platform/*` is the plugin/browser adapter layer.
- Production source must not contain mock fallback modes. The repository still exposes an
  explicit `create_analysis_demo_case` development/audit entry point; it seeds public-small
  fixtures only when explicitly requested and is not part of normal import fallback.
- Tests may use `vi.mock`, but test doubles must not leak into runtime files.

## Guarded Rules

`scripts/check-frontend-runtime-guard.ps1` enforces:

- No direct Tauri imports outside API/event/platform adapters.
- Page shells do not import stores, API modules, platform adapters, or Tauri packages.
- Route pages cannot own React state/effects or import UI components. Every route must render a
  feature container, which prevents page-local presentation logic from returning.
- Shared components do not import stores, API modules, platform adapters, or Tauri packages, except layout shell whitelist.
- Feature components do not import stores, API modules, platform adapters, or Tauri packages; only feature hooks, `use-*-model.ts` files, and `containers/*` may do so.
- Feature components cannot runtime-import feature hooks or page models, and feature hooks/models cannot runtime-import UI modules. Type-only contracts do not create runtime ownership.
- Domain directories no longer live under shared `components/analysis`, `components/dashboard`, `components/import`, or `components/mcp`.
- Production source contains no mock/demo/example presentation residue.
- Runtime code does not hand-write raw `<input>`, `<textarea>`, `<select>`, or `<table>` outside
  documented exceptions.
- Runtime code does not hand-write raw `<button>` outside documented primitive/sidebar internals.
- Feature/page code cannot import low-level `@/app/components/ui/table`; use `DenseDataTable`
  or a semantic key/value summary instead.

## Current Domain Component Homes

| Domain | Component path |
|---|---|
| analysis | `frontend/src/features/analysis/components` |
| artifacts | `frontend/src/features/artifacts/components` |
| batch | `frontend/src/features/batch/components` |
| case | `frontend/src/features/case/components` |
| dashboard | `frontend/src/features/dashboard/components` |
| files | `frontend/src/features/files/components` |
| graph | `frontend/src/features/graph/components` |
| import | `frontend/src/features/import/components` |
| jobs | `frontend/src/features/jobs/components` |
| mcp | `frontend/src/features/mcp/components` |
| notebook | `frontend/src/features/notebook/components` |
| recovery | `frontend/src/features/recovery/components` |
| reports | `frontend/src/features/reports/components` |
| rule-packs | `frontend/src/features/rule-packs/components` |
| search | `frontend/src/features/search/components` |
| settings | `frontend/src/features/settings/components` |
| timeline | `frontend/src/features/timeline/components` |

## Acceptance Standard

- `pnpm --dir frontend typecheck`
- `pnpm --dir frontend lint`
- `pnpm --dir frontend test`
- `powershell -ExecutionPolicy Bypass -File scripts/check-frontend-runtime-guard.ps1`
- `git diff --check`
