# Dependency Advisory Policy

This document defines the severity grading, response timeline, exception process, and review cadence for dependency security advisories in Forensics Workbench.

The policy applies to all Rust crate dependencies in the workspace. Advisories are sourced from the [RustSec Advisory Database](https://github.com/rustsec/advisory-db) and checked via `cargo-deny`.

## Severity Grading

Each advisory is assigned a severity level based on its CVSS v3 score (when available) or an equivalent qualitative assessment. If the RustSec advisory does not carry a CVSS score, the project maintainer assigns severity by evaluating exploitability, impact, and exposure in the context of this codebase.

| Severity | CVSS Range | Criteria |
|----------|-----------|----------|
| Critical | 9.0 -- 10.0 | Remote unauthenticated code execution; sandbox escape; cryptographic breaks that expose case data or host filesystem. |
| High     | 7.0 -- 8.9 | Local privilege escalation; authenticated remote code execution; information disclosure of protected evidence; denial of service that corrupts case database. |
| Medium   | 4.0 -- 6.9 | Parsing panics in evidence readers; uncontrolled resource consumption; timing side-channels in search index. |
| Low      | 0.1 -- 3.9 | Information disclosure of non-sensitive metadata; best-practice violations without a practical attack vector in this application. |

**Severity override**: A maintainer may raise (never lower) the severity if the advisory's stated impact class matches a critical data flow in this application (e.g., NTFS parsing, EVTX decoding, evidence media streaming).

## Response Timeline

Timelines start from the moment the advisory is detected by CI (`check-dependency-security.ps1` failure) or reported in the RustSec database.

| Severity | Response SLA | Action |
|----------|-------------|--------|
| Critical | 24 hours | Immediate patch or mitigation merge. If upstream fix is unavailable, ship a config-level block (`[bans].deny`) with an exception note and escalate to the dependency owner. |
| High     | 7 days | Evaluate upgrade path; if blocked on upstream, document exception in `deny.toml` with an expiry no later than 30 days out. |
| Medium   | 30 days | Plan upgrade in the next scheduled dependency refresh cycle. Document exception if the fix is still pending at day 30. |
| Low      | 90 days | Address during routine maintenance. Exception may be renewed once without escalation. |

## Exception Process

When an advisory cannot be resolved within its SLA:

1. **Document**: Add an entry to `[advisories].ignore` in `deny.toml` following the exception template. Each entry must include:
   - `owner`: the person or team accountable for resolution
   - `expires`: an ISO-8601 date (`YYYY-MM-DD`) by which the exception must be re-evaluated
   - `reason`: a technical justification explaining why the fix is blocked and what upstream conditions must change

2. **Approve**: Exceptions for Critical or High severity require approval via a pull request with at least one other maintainer review. Medium and Low exceptions may be self-approved but must still follow the template.

3. **Expire**: The `check-deny-exceptions.ps1` guard script fails CI when any exception date has passed. Expired exceptions must be removed or renewed with an updated justification and expiry date. Renewal of a Critical/High exception requires a new review.

4. **Track**: All active exceptions are visible in `deny.toml`. The governance JSON snapshot at `testdata/governance/v2-security-taxonomy.json` records the dependency-security category for audit purposes.

## Review Cadence

| Activity | Frequency | Owner |
|----------|-----------|-------|
| Automated `cargo deny check` (all categories) | Every CI run | `check-dependency-security.ps1` |
| Exception expiry enforcement | Every CI run | `check-deny-exceptions.ps1` |
| Full dependency audit (human review of all advisories, bans, licenses, sources) | Monthly | Project maintainer rotation |
| Severity classification review (re-assess CVSS context for active advisories) | Monthly | Project maintainer |

The monthly audit produces a brief summary appended to the governance snapshot, noting the number of active advisories, new exceptions granted, expired exceptions closed, and any upstream dependency refresh actions taken.

## Tools

| Tool | Purpose |
|------|---------|
| `cargo-deny` | Rust dependency checker (advisories, bans, licenses, sources) |
| `deny.toml` | Configuration and exception registry |
| `scripts/check-dependency-security.ps1` | CI runner that executes all four `cargo deny check` categories and emits a structured JSON report |
| `scripts/check-deny-exceptions.ps1` | Validates exception format (owner, expires, reason) and enforces expiry dates |
| `testdata/governance/v2-security-taxonomy.json` | Governance snapshot with dependency-security category |
