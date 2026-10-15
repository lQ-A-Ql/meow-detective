# Dependency Decisions

This document records non-trivial dependency license and sourcing decisions for the Forensics Workbench workspace. Each entry explains why a license was added to the `deny.toml` allow-list or why a dependency was accepted despite an advisory/ban.

## License decisions

### CDLA-Permissive-2.0

- **Date**: 2026-06-21
- **Owner**: Registry Quality Maintenance
- **Crates**: `webpki-root-certs`, `webpki-roots`
- **License**: CDLA-Permissive-2.0
- **Decision**: Allowed
- **Reason**: These crates ship Mozilla/webpki root certificate data as a data bundle. The CDLA-Permissive-2.0 license is a data-specific permissive license that permits unrestricted use, modification, and distribution. The data is not executable code and does not impose copyleft or network-use obligations. It is pulled transitively by `rustls` / `tauri` networking stacks.
- **Review cadence**: Re-evaluate on each monthly dependency audit or when the root-cert data crate license changes.

## Advisory review — 2026-10-16

Owner: Dependency Maintenance. The review preserves advisory checking for both configured targets; it does not change Cargo.lock or the default unmaintained-advisory enforcement.

### Withdrawn GTK3 advisories

Remove RUSTSEC-2024-0411 through RUSTSEC-2024-0420 from the exception list. The RustSec database records withdrawal on 2026-08-14 after GTK3 development resumed. The current cargo-deny scan reports all ten as advisory-not-detected; retaining them would conceal obsolete policy entries. See [RustSec GTK3 advisory](https://rustsec.org/advisories/RUSTSEC-2024-0419.html).

`unused-ignored-advisory = "deny"` now fails when an exception stops matching the dependency graph. This follows the [cargo-deny advisory configuration](https://embarkstudios.github.io/cargo-deny/checks/advisories/cfg.html#unused-ignored-advisory).

### Remaining unmaintained dependencies

| Advisory | Resolved dependency | Exposure and disposition |
|---|---|---|
| [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) | proc-macro-error 1.0.4 | Linux-only build dependency of glib-macros 0.18.5 / gtk3-macros 0.18.2 in Tauri's GTK path; not present in the Windows target graph. |
| [RUSTSEC-2025-0075](https://rustsec.org/advisories/RUSTSEC-2025-0075.html) | unic-char-range 0.9.0 | rust-unic family reached through unic-ucd-ident -> urlpattern 0.3.0 -> tauri-utils 2.9.3. |
| [RUSTSEC-2025-0080](https://rustsec.org/advisories/RUSTSEC-2025-0080.html) | unic-common 0.9.0 | Same transitive URL-pattern/configuration path. |
| [RUSTSEC-2025-0081](https://rustsec.org/advisories/RUSTSEC-2025-0081.html) | unic-char-property 0.9.0 | Same transitive URL-pattern/configuration path. |
| [RUSTSEC-2025-0098](https://rustsec.org/advisories/RUSTSEC-2025-0098.html) | unic-ucd-version 0.9.0 | Same transitive URL-pattern/configuration path. |
| [RUSTSEC-2025-0100](https://rustsec.org/advisories/RUSTSEC-2025-0100.html) | unic-ucd-ident 0.9.0 | Same transitive URL-pattern/configuration path. |

These are informational unmaintained advisories without patched releases, rather than findings with a published CVSS score. Project classification: **Low maintenance risk** for the reviewed dependency paths. This is a contextual exposure assessment, not a claim that the crates are free of vulnerabilities.

The rust-unic path is part of Tauri capability/configuration URL-pattern handling, not evidence decoding. The shipped default capability is local to the main application window and declares no remote capability URLs. Reassess severity before introducing untrusted/user-provided remote capability patterns, using these crates in evidence parsing, or enabling another dependency path.

Under the [dependency advisory policy](dependency-advisory-policy.md), this is the first self-approved Low-severity renewal. All six exceptions name Dependency Maintenance and expire on **2026-11-15** (30 days after review). At the next review, replace the upstream dependency path or escalate any further renewal; do not silently extend the dates again. Re-review the GTK build path for any Linux desktop distribution.

Validation uses `cargo tree --locked --target x86_64-pc-windows-msvc -i unic-ucd-ident`, `cargo tree --locked --target x86_64-unknown-linux-gnu -i proc-macro-error`, the current RustSec database, `cargo deny check advisories bans licenses sources`, and `scripts/check-deny-exceptions.ps1`.
