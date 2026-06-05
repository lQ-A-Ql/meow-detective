# Task 5 Quality Gate Baseline

Date: 2026-06-04

Purpose: record the exact quality gate commands and script availability for Task 19 final execution without running the full gates during this baseline task.

## Sources Verified

- `README.md`: documents backend tests, frontend tests, and quality gates.
- `AGENTS.md`: documents backend, frontend, and Tauri command working directories.
- `Cargo.toml`: confirms the repository root is a Cargo workspace and includes `crates/transport` plus `apps/desktop/src-tauri`.
- `frontend/package.json`: confirms frontend scripts `typecheck`, `lint`, `test`, `test:coverage`, and `build` exist.

## Rust Gates

Run from repository root `D:\forensics`:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Optional documented build check for the Tauri shell crate, from repository root `D:\forensics`:

```bash
cargo build -p forensics-desktop
```

## Frontend Gates

Run from working directory `frontend/`:

```bash
pnpm typecheck
pnpm lint
pnpm test
pnpm build
```

Script availability in `frontend/package.json`:

- `typecheck`: `tsc --noEmit`
- `lint`: `eslint src/`
- `test`: `vitest run`
- `build`: `vite build`
- `test:coverage`: `vitest run --coverage`

## Targeted Wave 1 Commands

Use these for focused checks before the final full gates when relevant:

```bash
cargo test -p transport
cargo test -p forensics-desktop import::pipeline::tests
```

Run frontend targeted Vitest files from working directory `frontend/`:

```bash
pnpm test -- <test-file>
```

Example pattern only, not a discovered npm script: `pnpm test -- src/path/to/example.test.tsx`.

## Notes For Task 19

- Do not run frontend commands from the repository root; all listed frontend gate commands use `frontend/` as the working directory.
- The README quality gate list includes frontend `typecheck`, `lint`, and `test`; `pnpm build` is separately documented in README/AGENTS and confirmed in `frontend/package.json`.
- No full quality gates were run for this task. This file is a baseline inventory only.
- No missing required frontend scripts were found.
