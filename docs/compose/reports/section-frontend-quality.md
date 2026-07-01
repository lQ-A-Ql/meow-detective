# Section 4: Frontend Code Quality Analysis

## Summary of Frontend Quality Posture

- The frontend is a **React 18 + TypeScript 5.x + Vite 6** application with strict TypeScript enabled, centralized path aliases (`@/`), and a clean separation between the API layer (`frontend/src/lib/api/`), feature hooks (`frontend/src/features/*/hooks.ts`), and Zustand stores (`frontend/src/stores/`).
- **No direct Tauri `invoke` calls leak outside the API layer**; every command is routed through `apiClient.request(...)` using centralized command constants in `frontend/src/lib/api/commands.ts`.
- **Server state is managed with TanStack Query** and **client state with Zustand**, with current Vitest coverage exceeding the configured thresholds across all four dimensions (lines, statements, functions, branches).
- Page components remain within the 500-line policy limit, but the largest page (`V3Dashboard.tsx`) is within 30 lines of the boundary, and several auxiliary files (`use-file-browser.ts`, `mcp-store.ts`) are large enough to warrant attention.

## Metrics Table

| Category | Metric | Observed Value | Notes |
|---|---|---|---|
| **Pages** | Non-test `.tsx` files in `frontend/src/app/pages/` | **17** | Includes sub-pages under `settings/`; excludes helpers like `use-file-browser.ts`. |
| **Page size** | Largest page file | **471 lines** (`V3Dashboard.tsx`) | Under the 500-line component limit, but close to it. |
| **Pages > 500 lines** | Count | **0** | All page components comply with the policy. |
| **API layer** | Non-test files in `frontend/src/lib/api/` | **16** | Includes `client.ts`, `commands.ts`, and 14 domain files. |
| **API layer** | Test files in `frontend/src/lib/api/` | **15** | Good coverage of the API contract. |
| **Stores** | Zustand stores in `frontend/src/stores/` | **4** | `ui-store.ts`, `analysis-store.ts`, `mcp-store.ts`, `selection-store.ts`. |
| **Test files** | Total `*.test.{ts,tsx}` in `frontend/src` | **76** | Slightly higher than the 71/75 figures in earlier notes; reflects recent additions. |
| **Direct `invoke` usage** | Files outside `frontend/src/lib/api/` | **0** | Only `frontend/src/lib/api/client.ts` imports `invoke`. |
| **TanStack Query** | Files referencing `useQuery`/`useMutation`/`QueryClient` | **~204 matches** | Used consistently for server-state caching and invalidation. |
| **Coverage** | Lines / Statements / Functions / Branches | **65% / 64.37% / 60.36% / 55.61%** | Measured via `pnpm test:coverage` on audit date. Exceeds Vitest thresholds of 45% / 45% / 45% / 35%. |
| **Quality gates** | Latest lint / typecheck / guard scripts | **PASS** | `pnpm lint`, `pnpm typecheck`, and relevant PowerShell guard scripts pass. |

## Highlighted Strengths

1. **Centralized API and command contract.** `frontend/src/lib/api/client.ts` is the single place that calls `invoke`, and `frontend/src/lib/api/commands.ts` defines all 96-ish Tauri command names as typed constants. This makes renaming or auditing command usage straightforward and prevents string-typo regressions.

2. **Clean architecture layering.** Feature hooks (`frontend/src/features/case/hooks.ts`, `frontend/src/features/files/hooks.ts`, etc.) wrap TanStack Query, while UI components consume hooks and stores. The `apiClient` is never invoked from pages or components directly.

3. **TypeScript strictness.** `tsconfig.json` enables `strict: true`, `noEmit: true`, `isolatedModules: true`, `forceConsistentCasingInFileNames: true`, and bundler module resolution, providing a solid foundation for type safety.

4. **State-management separation.** Server state (cases, files, jobs, artifacts, search, timeline) is handled by TanStack Query with explicit cache invalidation (e.g., `qc.invalidateQueries({ queryKey: ['case'] })`), while local UI state (navigation, selection, analysis panel progress) lives in Zustand stores.

5. **Coverage above thresholds.** Current v8 coverage sits at **65% lines, 64.37% statements, 60.36% functions, 55.61% branches**, measured via `pnpm test:coverage` during the audit and comfortably above the 45/45/45/35 thresholds defined in `vitest.config.ts`.

6. **Small, focused layout components.** `frontend/src/components/layout/AppShell.tsx` is only 13 lines and merely composes `TopBar`, `BottomDrawer`, and children, demonstrating good layout/component decomposition.

## Risks and Issues Found

1. **Page size boundary pressure.** `frontend/src/app/pages/V3Dashboard.tsx` is **471 lines** and `frontend/src/app/pages/DataAnalysis.tsx` is **349 lines**. While both remain under the 500-line limit, V3Dashboard is close enough that adding a new dashboard section would push it over. The file is also JSX-heavy, making it harder to unit-test individual sections in isolation.

2. **Large auxiliary files in the pages directory.** `frontend/src/app/pages/use-file-browser.ts` is **464 lines**, and `frontend/src/app/pages/file-tree-utils.ts` is **48 lines**. These are not page components but co-located page helpers; the hook in particular mixes keyboard handling, selection, sorting, and virtual scrolling logic and could be split into smaller, reusable hooks or moved to `frontend/src/features/files/`.

3. **Large store file.** `frontend/src/stores/mcp-store.ts` is **311 lines** and contains MCP server connection, resource/tool/prompt caching, and error handling. This is a high-risk area for future regressions because it bundles transport, protocol, and UI state in one file.

4. **Low per-file coverage in several areas.** Although aggregate coverage passes, individual files are poorly exercised:
   - `frontend/src/app/pages/CaseActions.tsx` — **4.76% lines, 7.69% functions**.
   - `frontend/src/components/notebook/NotebookEntryForm.tsx` — **17.97% lines, 12.82% functions**.
   - `frontend/src/components/gql/GqlAutocomplete.tsx` — **0% coverage**.
   - `frontend/src/components/gql/GqlQueryInput.tsx` — **31.03% lines, 36.36% functions**.
   - `frontend/src/components/gql/GqlResultView.tsx` — **43.75% lines, 40% functions**.
   - Settings sub-sections (`McpSection.tsx`, `PreviewSection.tsx`, `ImportPerformanceSection.tsx`) are also under 40% lines.

5. **Very large test files.** Test files are exempt from the 500-line component limit, but three page tests exceed 600 lines:
   - `frontend/src/app/pages/V2Workbench.test.tsx` — **756 lines**.
   - `frontend/src/app/pages/DataAnalysis.test.tsx` — **675 lines**.
   - `frontend/src/app/pages/FileBrowser.test.tsx` — **627 lines**.
   Long tests tend to be brittle and slow to debug; they may benefit from shared fixtures or page-object helpers.

6. **Low coverage thresholds.** The Vitest thresholds (`lines: 45`, `statements: 45`, `functions: 45`, `branches: 35`) are permissive. While current coverage exceeds them, they do not strongly constrain future regressions, especially in branches.

7. **Tailwind 4 CSS-first migration is active but custom colors remain inline.** Several files (e.g., `V3Dashboard.tsx`) use arbitrary hex values like `bg-[#fafafa]` and `text-[#111]`. These are not theme-tokenized and could drift from the design system. Tailwind 4 is configured via `@tailwindcss/vite` without a `tailwind.config.js`, so design tokens should live in CSS theme files rather than inline hex literals.

## Improvement Recommendations

### P0 — Address before next release

- **Split `frontend/src/app/pages/V3Dashboard.tsx`** before it crosses the 500-line component limit. Extract each dashboard section (Graph, Data Sources, Timeline, Artifacts, Correlation, Platform Coverage, Rule Packs, Batch Status) into small components under `frontend/src/components/dashboard/` or `frontend/src/app/pages/dashboard/`. This also improves testability and React re-render performance.
- **Add coverage for `CaseActions.tsx`** and the GQL notebook components (`GqlAutocomplete.tsx`, `GqlQueryInput.tsx`, `GqlResultView.tsx`). These are the lowest-covered production files and are the most likely to hide regressions.

### P1 — Within the next sprint

- **Refactor `frontend/src/app/pages/use-file-browser.ts` (464 lines)** into focused hooks under `frontend/src/features/files/hooks.ts` or a dedicated `frontend/src/features/files/hooks/` directory, separating keyboard navigation, selection, sorting, and virtual-tree concerns.
- **Refactor `frontend/src/stores/mcp-store.ts` (311 lines)** into smaller stores or helper modules: one for MCP server configuration, one for connection state, and one for cached resources/tools/prompts. This reduces the blast radius of MCP transport changes.
- **Raise Vitest coverage thresholds** from `45/45/45/35` to at least `55/55/55/45` to prevent future regressions, aligning with the current actual coverage rather than leaving a large margin.
- **Standardize Tailwind color usage** by replacing inline arbitrary hex values in dashboard and analysis pages with theme CSS variables from `frontend/src/styles/theme.css` or `tailwind.css`.

### P2 — Quality-of-life and technical debt

- **Create shared test fixtures / page objects** for the three oversized page tests (`V2Workbench.test.tsx`, `DataAnalysis.test.tsx`, `FileBrowser.test.tsx`) to reduce duplication and improve maintainability.
- **Add an ESLint rule or guard script** that warns when a production `.tsx` file exceeds 400 lines, giving an early warning before the 500-line hard limit is reached.
- **Document the frontend state-management convention** (TanStack Query for server state, Zustand for local UI state) in `AGENTS.md` or a frontend-specific `README.md` so new feature work follows the pattern consistently.
- **Consider adding a visual regression or component-story harness** for the low-covered UI components (settings sections, notebook forms, GQL widgets) to complement unit tests and catch unintended UI changes.
