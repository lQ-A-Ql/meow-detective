# Section 5: Architecture and Data Flow Analysis

## Summary

Forensics Workbench is a **backend-led, layered Tauri desktop application**. A Rust workspace of 37 crates (36 library crates plus the Tauri shell) performs evidence processing, while a React 18 frontend renders the investigator UI. The architecture enforces strict dependency direction: domain and transport crates sit at the bottom, parser/core crates feed application services, and Tauri commands act as thin IPC adapters. All state is centralized in `AppState`, and the frontend/backend contract is expressed through manually-synchronized DTOs and event topics in `crates/transport`.

This section analyzes the crate layering, the request/event paths, the DTO contract, the media-preview security boundary, and the MCP policy boundary. It identifies the project's architectural strengths and its principal long-term risk: the manual IPC contract between Rust and TypeScript.

## Architecture Layers

```mermaid
graph TB
    subgraph Frontend
        UI[React 18 + Vite + Tailwind 4]
        API[apiClient.request@frontend/src/lib/api/client.ts]
        EVT[EventBus@frontend/src/lib/events/bus.ts]
    end

    subgraph Tauri_Shell[apps/desktop/src-tauri]
        CMD[96 Tauri commands<br/>apps/desktop/src-tauri/src/commands/]
        STATE[AppState<br/>state::AppState]
        MEDIA[evidence-media:// protocol handler<br/>src/media_protocol.rs]
    end

    subgraph App_Services[crates/app-services]
        SVC[use-case services<br/>file_service, case_service, ...]
    end

    subgraph Transport[crates/transport]
        DTO[DTOs: 33 domain files<br/>src/dto/]
        EVT_RUST[EventTopic enum<br/>src/events/mod.rs]
        ERR[CommandError / ApiErrorDto]
        REQ[command request structs<br/>src/commands/mod.rs]
    end

    subgraph Persistence[crates/persistence-sqlite]
        REPO[15 repositories]
        MIG[33 migrations]
        DB[(SQLite + WAL)]
    end

    subgraph Core[Core / Parser Crates]
        EVID[evidence-core]
        FS[fs-ntfs / fs-fat / fs-exfat / fs-ext4 / fs-xfs / fs-btrfs / fs-apfs / fs-hfsplus]
        ART[artifacts-windows / linux / macos]
        SRC[search / timeline / catalog / exchange]
    end

    subgraph Domain[crates/domain]
        ENT[CaseId / FileEntryId / DataSourceId / Artifact / TimelineEvent]
    end

    UI --> API
    API --> CMD
    CMD --> SVC
    SVC --> DTO
    SVC --> REPO
    SVC --> EVID
    SVC --> FS
    SVC --> ART
    SVC --> SRC
    REPO --> DB
    EVID --> Domain
    FS --> Domain
    ART --> Domain
    SRC --> Domain
    DTO --> Domain
    CMD --> STATE
    CMD --> EVT_RUST
    EVT --> EVT_RUST
    MEDIA --> SVC
    MEDIA --> STATE
```

| Layer | Responsibility | Key files |
|---|---|---|
| Frontend | Investigator UI, API client, event subscriptions | `frontend/src/app/pages/`, `frontend/src/lib/api/client.ts`, `frontend/src/lib/events/bus.ts` |
| Tauri commands | Thin IPC adapters, input validation, state access, audit logging | `apps/desktop/src-tauri/src/commands/*.rs`, `apps/desktop/src-tauri/src/lib.rs` |
| Application services | Per-domain use-case orchestration | `crates/app-services/src/*_service.rs` |
| Transport | DTOs, command requests, event topics, error taxonomy | `crates/transport/src/dto/`, `crates/transport/src/commands/`, `crates/transport/src/events/` |
| Persistence | SQLite repositories, migrations, WAL pragmas | `crates/persistence-sqlite/src/repositories/*_repo.rs`, `src/migrations/scripts/*.sql` |
| Core / parsers | Evidence readers, filesystem parsers, artifact extractors, search/timeline engines | `evidence-core`, `fs-*`, `artifacts-*`, `search`, `timeline`, `catalog`, `exchange` |
| Domain | Core entities and identifiers | `crates/domain/src/` |

## Dependency Direction and Layering

The workspace follows a strict **inward-pointing dependency** rule:

1. `domain` and `transport` are the bottom layer. Every crate that serializes data or returns errors depends on `transport`.
2. `persistence-sqlite`, `evidence-core`, `fs-*`, `artifacts-*`, `search`, `timeline`, `catalog`, `reports`, and `exchange` depend on `domain` and sometimes `transport` for DTOs/errors, but **never** on Tauri or the frontend.
3. `app-services` consumes persistence and core crates, and returns `transport` DTOs.
4. Tauri commands (`apps/desktop/src-tauri`) consume `app-services` and `transport`, and are the only layer that touches the Tauri runtime and the desktop window state.

This direction was verified by inspecting `Cargo.toml` files for representative parser crates:

- `crates/evidence-core/Cargo.toml` depends on `serde`, `chrono`, `thiserror`, `anyhow` — no Tauri or frontend.
- `crates/fs-ntfs/Cargo.toml` depends on `evidence-core`, `serde`, `chrono`, `thiserror` — no Tauri or frontend.
- `crates/artifacts-windows/Cargo.toml` depends on `domain`, `artifacts-core`, `evtx`, `rusqlite`, crypto crates — no Tauri or frontend.

The rule is also enforced by repository guard scripts, notably `check-command-sql-boundary.ps1` and `check-media-protocol-guard.ps1`, which prevent raw SQL from leaking into command handlers and ensure media preview stays on the `evidence-media:` protocol.

## DTO and Event Contract

### DTOs

The frontend/backend contract is defined in `crates/transport/src/dto/`. `crates/transport/src/dto/mod.rs` re-exports 33 domain modules and hundreds of DTO types, all following the convention:

- Type names end in `Dto` on the Rust side (e.g., `FileEntryRowDto`).
- `#[serde(rename_all = "camelCase")]` serializes fields for TypeScript.
- `#[serde(skip_serializing_if = ...)]` omits optional/false values.

The TypeScript mirror lives in `frontend/src/types/`, re-exported from `frontend/src/types/models.ts`. This is a **manual mirror**: there is no code generation. Adding a field to `crates/transport/src/dto/files.rs` without updating `frontend/src/types/files.ts` will compile on both sides but can fail at runtime.

### Events

Backend→frontend events are typed in `crates/transport/src/events/mod.rs`:

- 19 string constants (`TOPIC_*`) and a matching `EventTopic` enum.
- `EventTopic` serializes as kebab-case (`"job-progress"`, `"import-phase-progress"`, etc.).
- `EventEnvelope<T>` wraps every event with `event_id`, `topic`, `ts`, and `payload`.

The frontend mirror is `frontend/src/types/events.ts`:

```typescript
export type EventTopic =
  | 'case-opened'
  | 'case-closed'
  | 'job-created'
  | 'job-started'
  | 'job-progress'
  ...
```

The event contract is tested in Rust (e.g., `event_topic_serializes_as_wire_topic`), but there is no automated check that the TypeScript union stays in sync with the Rust enum.

### Command Requests

Request DTOs live in `crates/transport/src/commands/mod.rs`. They carry validation methods such as `validate()` and `validate(&mut self)`, and enforce constraints like:

- `MAX_PAGE_LIMIT = 500` for file/search/timeline pagination.
- Import source paths reject Windows device paths (`\\.\`, `\\?\`), null bytes, and reserved device names.
- Export destination paths reject device paths and default to `overwrite = false`.

## Request Flow Walkthrough

The file-browsing path is representative of the full request flow.

### 1. Frontend API client

`frontend/src/lib/api/client.ts` wraps Tauri's `invoke` and normalizes errors to `ApiErrorDto`:

```typescript
class ApiClient {
  async request<T>(command: string, payload?: Record<string, unknown>) {
    try {
      return await invoke<T>(command, payload);
    } catch (error) {
      throw toApiError(error, `COMMAND_${command.toUpperCase()}_FAILED`);
    }
  }
}
```

Pages and hooks must use `apiClient.request`; direct `invoke` calls are not allowed by project convention.

### 2. Tauri command

`apps/desktop/src-tauri/src/commands/file_commands.rs` exposes commands such as `get_file_rows_request`. Each command:

1. Validates the request (`request.validate()`).
2. Locks `AppState` and checks for an active case.
3. Opens a fresh SQLite connection via `command_support::get_case_connection`.
4. Spawns the blocking work onto `tauri::async_runtime::spawn_blocking`.
5. Delegates to `app_services::file_service` and maps service errors to `CommandError`.

Example excerpt from `get_file_rows_request`:

```rust
pub async fn get_file_rows_request(
    state: State<'_, AppState>,
    mut request: GetFileRowsRequest,
) -> Result<FileRowsPageDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = get_case_connection(&app_state)?;
        file_service::get_file_rows_for_request(&conn, &request)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
```

### 3. Application service

`crates/app-services/src/file_service/mod.rs` is the public entry point. It re-exports sub-modules for tree queries, file rows, enumeration, export, MFT handling, and preview. For example, `get_file_rows_for_request` sorts the full result set and then paginates, while `get_file_jump_context` resolves a target file's directory context and ancestor chain.

### 4. Repository / SQLite

The service calls repositories such as `persistence_sqlite::repositories::file_repo::FileRepo`. The persistence layer runs 33 migration scripts and opens each connection with WAL pragmas (`journal_mode=WAL`, `foreign_keys=ON`, `busy_timeout=30000`, `synchronous=NORMAL`).

### 5. Error return path

Service errors are typed with `thiserror` (e.g., `FileServiceError`). The command layer converts them through `CommandError::from_service_error` into `ApiErrorDto` with `code`, `message`, `category`, `details`, and `recoverable`. The frontend receives a structured error that the UI can render or retry on.

## Event Flow Walkthrough

1. Backend services and task manager emit typed events using constants from `crates/transport/src/events/mod.rs`.
2. Tauri pushes these events to the webview.
3. Frontend `EventBus` (`frontend/src/lib/events/bus.ts`) subscribes by `EventTopic` and routes payloads to listeners or Zustand stores.

Event topics cover case lifecycle, job lifecycle, import progress, artifact additions, timeline updates, search index progress, and cache status. The 19 topics are listed in `crates/transport/src/events/mod.rs` and mirrored in `frontend/src/types/events.ts`.

## Security Boundaries

### Read-only evidence

The core forensic invariant is that **original evidence sources are never modified**. All writes are limited to:

- The case workspace directory.
- The SQLite case database.
- Search/timeline index directories.
- Explicit user export paths.

Import and preview operations open evidence readers in read-only mode. File extraction writes to a destination chosen by the investigator, with `overwrite=false` by default and a conflict returned if the target exists.

### Path validation

Path validation happens at the command boundary before any I/O:

- `validate_import_source_path` rejects null bytes, `\\.\` device paths, `\\?\` extended paths, and reserved Windows names (`CON`, `PRN`, `AUX`, `NUL`, `COM*`, `LPT*`).
- `safe_relative_path` in `file_service` rejects `..`, URL-encoded traversal, absolute paths, and null bytes.
- `validate_export_destination_path` rejects device paths for exports.

### Media protocol

Media preview is the most sensitive path because it exposes evidence bytes to the webview. The design avoids leaking host filesystem paths:

- `get_media_url` returns either a bounded `data:` URL (for files under `MAX_INLINE_MEDIA_PREVIEW_BYTES`) or an `evidence-media://handle/<encoded>` URL.
- The custom protocol handler (`apps/desktop/src-tauri/src/media_protocol.rs`) resolves the encoded handle through the runtime cache, validates that the handle belongs to the active case, and streams a bounded byte range.
- The CSP in `tauri.conf.json` explicitly allows `media-src 'self' data: evidence-media:`; no `file:` or `asset://` fallback is permitted.
- Range requests are clamped to `MAX_VIEWER_RANGE_LENGTH` (1 MB) and validated against the file size before reading.

### MCP policy

`AppState` holds MCP configuration and live clients. The default permission profile is least-privilege:

- `resourceAccess = readOnly`
- `toolAccess = disabled`
- `promptAccess = readOnly`
- `networkPolicy = localhostOnly`

MCP server configurations are validated on load/save, stale clients are pruned when the config changes, and SSE transports only allow `http/https` without embedded credentials. MCP outputs entering the UI or reports must preserve source boundaries.

## Strengths

1. **Strict layering and dependency direction**. Parser and core crates have no Tauri or frontend dependencies, making them testable in isolation and reusable outside the desktop app.
2. **Thin command layer**. Tauri commands are focused on validation, state access, and delegation; business logic lives in `app-services` and below.
3. **Centralized state**. `AppState` owns the active case, task manager, MCP clients, and runtime cache. A single mutex per concern prevents accidental cross-case races and makes resource lifetimes explicit.
4. **Typed, structured errors**. `ApiErrorDto` crosses the IPC boundary with forensic categories (`validation`, `parser`, `security`, `external`, `timeout`, `internal`) and a `recoverable` flag, rather than raw strings or stack traces.
5. **Defense-in-depth media preview**. The combination of scoped handles, a custom protocol, CSP allow-listing, and bounded range reads keeps evidence paths out of the frontend.
6. **Repository guard scripts**. Automated checks (`check-command-sql-boundary`, `check-media-protocol-guard`, `check-release-guard`) encode architectural and security boundaries in CI.

## Risks

1. **Manual DTO/event synchronization is the single largest source of drift risk**. There is no code generation or contract test linking `crates/transport/src/dto/` to `frontend/src/types/`. A renamed Rust field that is not mirrored in TypeScript will fail at runtime.
2. **EventBus and transport event types are manually mirrored**. `frontend/src/types/events.ts` must match every Rust `EventTopic` variant and payload shape.
3. **AppState mixes concerns**. It holds case state, task management, MCP state, and runtime cache in one struct. While currently manageable, this centralizes too many dependencies as the surface grows.
4. **Error classification is partially string-based**. `CommandError::from_service_error` maps some service errors to categories by inspecting error messages, which is brittle for new error variants.
5. **No isolated worker process**. Long-running tasks are managed by `TaskManager` but still run on the Tauri-managed Tokio runtime; a CPU-bound ingest or indexing job can starve the UI command handler.
6. **MCP is a controlled extension channel with elevated trust**. A misconfigured MCP server (e.g., `toolAccess` elevated, `networkPolicy` relaxed) could expose the host to arbitrary tool execution or external network access.

## Improvement Recommendations

### P0 — Before V2 release

1. **Add an IPC contract regression test**. Generate JSON samples for every Rust DTO and event payload, and assert that the TypeScript types and runtime validators in `frontend/src/types/` accept them. This can be a CI step that runs a small Rust exporter and a TypeScript parser.
2. **Freeze the event topic list and document payloads**. The 19 topics are sufficient for V2; add a `docs/ipc-event-contract.md` table listing each topic, its Rust payload type, and its TypeScript shape.
3. **Add a guard script that detects DTO drift**. Compare exported Rust DTO JSON schema (or field names) against the TypeScript interfaces and fail CI on mismatch.

### P1 — Near-term engineering debt

4. **Refactor `AppState` into focused sub-states** (e.g., `CaseState`, `TaskState`, `McpState`, `CacheState`) to reduce the cross-cutting dependency surface and improve testability.
5. **Replace substring-based error classification** with explicit `category()` methods on each `thiserror` service enum or with `From<ConcreteError> for CommandError` implementations.
6. **Add command audit logging** for security-sensitive operations: case create/delete, file extract, data-source delete, and MCP tool calls. The audit trail should include the case ID, action, resource ID, and outcome.
7. **Introduce typed command wrappers in the frontend**. Instead of `apiClient.request('get_file_rows_request', payload)`, generate per-command functions so the payload and return types are checked at compile time.

### P2 — Hardening and polish

8. **Evaluate a lightweight schema generator** such as `ts-rs` or `typeshare` for the core DTOs. If acceptable, generate TypeScript interfaces from Rust to eliminate manual mirroring; if not, document the explicit "no-codegen" policy and the drift-detection guard script.
9. **Document and test the media handle lifecycle**. Ensure handles are invalidated when the active case is closed or when the case workspace is removed, so stale handles cannot be reused.
10. **Consider isolating long-running tasks**. For V3 scheduling, evaluate a dedicated worker thread pool or a separate ingest process so that heavy indexing/artifact extraction cannot block the Tauri command loop.

(End of section)
