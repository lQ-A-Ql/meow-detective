# 审计收集到的原始数据

## 项目基础指标

- Workspace crates: 37 (apps/desktop/src-tauri + 36 library crates)
- Backend languages: Rust Edition 2021
- Frontend: React 18.3.1, TypeScript ~5.5, Vite 6.4.2, Tailwind 4.1.12, pnpm 10.25.0
- Tauri commands: 96 (from apps/desktop/src-tauri/src/lib.rs generate_handler!)
- Frontend pages: 17 non-test .tsx files in frontend/src/app/pages/
- Frontend test files: 71 *.test.{ts,tsx}
- API layer files: 17 domain files in frontend/src/lib/api/
- Guard scripts: 12 check-*.ps1 in scripts/
- Release scorecard: 81/100 Grade B (2026-06)
- Known limitations: 36 documented items in testdata/governance/v2-known-limitations.json

## 门禁运行结果（2026-06-30）

| 检查 | 命令 | 结果 |
|---|---|---|
| Rust fmt | `cargo fmt --all -- --check` | PASS (no output) |
| Frontend typecheck | `pnpm --dir frontend typecheck` | PASS |
| Frontend lint | `pnpm --dir frontend lint` | PASS |
| Command SQL boundary | `scripts/check-command-sql-boundary.ps1` | PASS |
| Dead code guard | `scripts/check-dead-code-allow-guard.ps1` | PASS |
| Media protocol guard | `scripts/check-media-protocol-guard.ps1` | PASS |
| Frontend lockfile policy | `scripts/check-frontend-lockfile-policy.ps1` | PASS |

## 已收集的关键文件

- Cargo.toml: workspace members 37, workspace dependencies centralized
- lib.rs: 96 Tauri commands, command-registration test
- frontend/package.json: deps listed
- docs/release-scorecard.md: 81/100 Grade B
- testdata/governance/v2-known-limitations.json: 36 limitations

## 待分析重点模块

- Search/catalog: crates/search, crates/catalog, frontend Search page
- Timeline: crates/timeline, frontend Timeline page, sourceObjectId correlation
- Forensic overview: artifacts-core, artifacts-windows, frontend CaseOverview/V3Dashboard/V3ScoreCards
