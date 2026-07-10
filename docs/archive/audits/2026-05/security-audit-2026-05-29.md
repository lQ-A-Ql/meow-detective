# Security Audit Report — Forensics Workbench Rust Backend

> Archived: 2026-05 audit snapshot. This document is not a current security baseline.

**Auditor**: Codex (lQ-A-Ql)
**Date**: 2026-05-29
**Scope**: Tauri command layer, app-services, transport DTOs, state management, Cargo dependencies
**App Version**: 0.1.0
**Previous Audit**: 2026-05-27 (see audit-remediation-plan.md)

---

## Executive Summary

This audit covers the full Rust backend of the Forensics Workbench Tauri 2 desktop app. Compared to the previous audit (2026-05-27), significant progress has been made: CommandError is now a structured serializable type with sanitized messages, tracing is initialized, and path traversal in create_file_reader_fn has been fixed with safe_relative_path() + canonicalize() + starts_with().

However, several issues remain or have been newly identified. The most critical is the lack of path sandboxing in delete_case, which allows arbitrary directory deletion.

## Summary Table

| Severity | Count | Key Areas |
|----------|-------|-----------|
| Critical | 1 | Arbitrary directory deletion |
| High | 2 | Case name injection, unrestricted import paths |
| Medium | 5 | Unsafe code, unbounded memory, missing pagination limits |
| Low | 4 | Error type leaks, ID validation, file integrity |
| Info | 3 | Fragile SQL patterns, path echo, rate limiting |

---

## Critical

### C-01: Arbitrary Directory Deletion via delete_case

**Files**:
- apps/desktop/src-tauri/src/commands/case_commands.rs:221-253
- crates/app-services/src/case_service.rs:96-124

**Description**: delete_case accepts a raw case_root string from the frontend and passes it directly to fs::remove_dir_all(). The only check is verifying case.json exists inside the target -- but any directory on the filesystem containing a case.json can be destroyed. There is no confinement to a safe cases root directory.

**Attack vector**: A compromised or manipulated frontend, or a tampered forensics-recent-cases.json, can point case_root to any directory.

**Impact**: Full data loss of arbitrary directories.

**Remediation**: Define a canonical cases root (e.g. %APPDATA%/ForensicsWorkbench/cases/). Validate case_root is a direct child via canonicalize() + starts_with(). Reject symlink escapes.

---

## High

### H-01: Path Traversal in create_case via Unvalidated name

**Files**:
- crates/app-services/src/case_service.rs:44-45
- apps/desktop/src-tauri/src/commands/case_commands.rs:28-44

**Description**: create_case does root.join(name) where name comes from request.name with no validation. Names like "../../etc" or path traversal sequences create directories outside the intended root.

**Impact**: Arbitrary directory creation + SQLite database file placement.

**Remediation**: Validate name with regex ^[a-zA-Z0-9_ -]{1,100}$. Reject path separators, .., and null bytes. Verify canonical parent after join.

---

### H-02: Unrestricted Source Path in import_data_source

**Files**:
- apps/desktop/src-tauri/src/commands/file_commands.rs:145-160

**Description**: The source_path from the frontend is used to open E01/raw disk images and enumerate logical directories without any path validation or confinement.

**Impact**: Can probe/read arbitrary filesystem files by importing them as "evidence."

**Remediation**: Validate the path exists as a regular file or directory. Consider using tauri_plugin_dialog for file selection instead of raw path input.

---

## Medium

### M-01: Unsafe Lifetime Transmute in MFT Reader

**File**: crates/app-services/src/file_service.rs:915-917

A borrowed &AtomicBool is cast to &'static via raw pointer for use in a spawned thread. The original reference lifetime is not structurally enforced to outlive the thread.

**Remediation**: Clone the Arc<AtomicBool> and pass ownership to the thread instead of transmuting a borrow.

---

### M-02: Unbounded Memory in Artifact Extraction

**File**: crates/app-services/src/artifact_service.rs:30-33

Entire files are read into memory with no size cap via read_to_end(). A single multi-GB file causes OOM.

**Remediation**: Add a max size check (e.g. 50 MB) before read_to_end. Skip or chunk-read larger files.

---

### M-03: Unbounded In-Memory File Collection During Import

**File**: crates/app-services/src/file_service.rs (run_post_import_pipeline)

All file entries are collected into a single Vec<FileEntry> for timeline projection. For large images, this exhausts memory.

**Remediation**: Process files in streaming batches via cursor-based DB queries.

---

### M-04: No Upper Bound on Timeline Query limit

**Files**:
- crates/transport/src/commands/mod.rs:93-100
- apps/desktop/src-tauri/src/commands/timeline_commands.rs

GetTimelineRequest.limit is u32 with no enforced maximum. Callers can request u32::MAX rows.

**Remediation**: Clamp limit to a maximum (e.g. 1000) in the command handler.

---

### M-05: get_file_tree Returns All Nodes Without Pagination

**File**: apps/desktop/src-tauri/src/commands/file_commands.rs:487-502

Returns the entire file tree as a flat Vec<FileTreeNodeDto>. For large disk images, millions of entries.

**Remediation**: Deprecate in favor of get_file_children (lazy-loading). Add a maximum entry count if kept.

---

## Low

### L-01: From<String> for CommandError Can Leak Internal Details

**File**: crates/transport/src/errors.rs:97-99

The blanket From<String> impl passes raw error strings (potentially containing paths, SQL, internal state) to the frontend. While most handlers use the safe from_service_error, this is a trap for future developers.

**Remediation**: Remove or change to always return a generic message.

---

### L-02: DataSourceSummaryDto Exposes Full Source Path

**File**: crates/transport/src/dto/case.rs:40

The complete host filesystem path of evidence sources is sent to the frontend.

**Remediation**: Return only filename or display-friendly path.

---

### L-03: No Input Validation on String ID Parameters

**Files**: Multiple command handlers

file_id, parent_id, data_source_id are passed without format validation (UUID check, max length). Safe from SQL injection due to parameterized queries, but wastes DB time on garbage input.

**Remediation**: Validate ID format at command layer (max length, alphanumeric/UUID regex).

---

### L-04: Recent Cases File Lacks Integrity Protection

**File**: apps/desktop/src-tauri/src/commands/case_commands.rs:278-336

forensics-recent-cases.json is plain JSON in %APPDATA%. Local attackers can modify entries to redirect case roots.

**Remediation**: Sign with HMAC or validate entries point to valid case directories before displaying.

---

## Info

### I-01: SQL IN Clause Uses format! -- Safe but Fragile

**File**: crates/persistence-sqlite/src/repositories/file_repo.rs:121-131

Placeholder generation uses format! but values are parameterized. Safe today, but fragile.

### I-02: remove_case_from_list Echoes User Path

**File**: apps/desktop/src-tauri/src/commands/case_commands.rs:200

Success message echoes request.case_root back. Minor inconsistency with error sanitization approach.

### I-03: No Rate Limiting on Commands

All Tauri commands are callable without throttling. Low priority for single-user desktop.

---

## Positive Observations

- CommandError design is solid: from_service_error, from_lock_error, from_join_error all log real errors and return generic messages
- All SQL uses parameterized queries via rusqlite::params![]
- safe_relative_path() + canonicalize() + starts_with() defends file viewing paths correctly
- Resource limits exist: ARTIFACT_EXTRACTION_LIMIT (500), TEXT_INDEX_LIMIT (1000), MAX_RANGE_LENGTH (1 MB)
- Tauri capabilities are minimal: only core:default, dialog:default, dialog:allow-open
- PRAGMA foreign_keys=ON on all connections

---

## Dependencies

No known-vulnerable versions identified from version constraints. Recommend periodic cargo audit.

---

## Remediation Priority

1. **Immediate**: C-01 -- sandbox delete_case
2. **Before release**: H-01 -- validate case name; H-02 -- validate import path
3. **Soon**: M-01 -- fix unsafe transmute; M-02/M-03 -- add memory limits
4. **Hardening**: M-04/M-05 -- pagination limits; L-01 through L-04 -- defense in depth
