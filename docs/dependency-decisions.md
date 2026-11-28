# Dependency Decisions

This document records non-trivial dependency license and sourcing decisions for the Forensics Workbench workspace. Each entry explains why a license was added to the `deny.toml` allow-list or why a dependency was accepted despite an advisory/ban.

## License decisions

### moka — emulation preflight result reuse

`source-reconstruction` uses `moka 0.12.16` with the `sync` feature and
default features disabled. Distribution selects MIT. Its notice is retained
in `crates/source-reconstruction/THIRD-PARTY-NOTICES`.

The synchronous cache coalesces concurrent fallible initialization through
`try_get_with`. The key binds the case, source, published generation manifest,
platform, evidence segment metadata and runtime epoch. A cache of 32 results
retains immutable preflight and guest-profile facts. Source identity is
checked before and after retrieval. Case retirement advances the epoch and
invalidates entries. Existing `lru` bounds request-local filesystem views.

The published API and MIT license were reviewed on 2026-11-26. Validation
uses actual imported images, concurrent production service calls and source
change rejection.

### flexi_logger — application runtime log files

The application uses `flexi_logger 0.31.10` with default features disabled.
Distribution selects MIT from its MIT OR Apache-2.0 license. The upstream
notice is retained in `crates/infrastructure/THIRD-PARTY-NOTICES`.

`FileLogWriter` supplies JSONL file output, numbered rotation and archive
cleanup. A bounded application-owned channel moves file operations to a
dedicated worker. The existing `tracing-subscriber` registry owns event
capture and span context. The writer operates in Direct mode and performs
explicit pre-write rotation so rotation errors reach application health.

The dependency is pinned in workspace dependencies and Cargo.lock. Validation
uses actual concurrent tracing events, filesystem rotation, Windows file
sharing errors, explicit reopen, and bounded-memory pressure tests.
The published crate manifest, MIT license and `FileLogWriter` implementation
were reviewed on 2026-11-26.

### indicatif — task progress estimates

The task-progress implementation uses `indicatif 0.18.6` under the MIT license.
The workspace dependency is consumed by `source-kernel`. Its hidden progress
bar supplies the smoothed processing rate and remaining-time estimate.
The task reporter provides stage weights, independent heartbeats, sequence
numbers and the measurement thresholds used by the desktop interface.

The resolved supporting packages are `console 0.16.6` and `unit-prefix 0.5.2`
under MIT, and `encode_unicode 1.0.0` under MIT OR Apache-2.0.
The license, advisory, source and dependency-ban checks passed on 2026-11-24.
The workspace resolves Tauri's `cookie` dependency to `0.18.3` for the current
advisory policy. Upstream references are the
[indicatif API](https://docs.rs/indicatif/0.18.6/indicatif/struct.ProgressBar.html),
[MIT license](https://github.com/console-rs/indicatif/blob/main/LICENSE) and
[RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0594.html).

### tinybmp — shared BMP decoding

- **Date**: 2026-11-23
- **Owner**: Image Inspection Maintenance
- **Version**: `0.7.0`, pinned registry source with a reviewed local patch.
- **License**: MIT OR Apache-2.0; distribution selects MIT, already permitted
  by `deny.toml`. Upstream copyright and both license files remain in
  `vendor/tinybmp-0.7.0`; the application notice is
  `crates/image-inspection/THIRD-PARTY-NOTICES`.
- **Use**: One native header parser serves bounded metadata inspection and
  RGB/index-domain row analysis. Raw indices, original row order and padding
  remain available through the upstream API.
- **Patch**: Expose metadata-only header access, validate color planes,
  palette size and pixel-data offset, and share checked layout-length
  calculation with `RawBmp`. Exact source provenance and modifications are in
  `vendor/tinybmp-0.7.0/LOCAL-PATCH.md`.

### sysinfo — process execution ownership

- **Date**: 2026-11-22
- **Owner**: Source Lifecycle Maintenance
- **Version**: `0.40.4`, `system` feature, default features disabled.
- **License**: MIT, covered by the existing `deny.toml` policy.
- **Use**: Query PID and process start time before reclaiming an abandoned
  persisted execution attempt. A targeted process refresh does not request
  command-line arguments, environment variables, CPU, memory, or task lists.
- **Redistribution**: Preserve the copyright and license notice in
  `crates/source-kernel/THIRD-PARTY-NOTICES` with source and binary distributions.
- **Validation**: Actual child-process exit detection and scoped task queue tests.

Sources: [upstream MIT license](https://github.com/GuillaumeGomez/sysinfo/blob/main/LICENSE),
[process identity API](https://docs.rs/sysinfo/0.40.4/sysinfo/struct.Process.html),
[targeted refresh API](https://docs.rs/sysinfo/0.40.4/sysinfo/struct.ProcessRefreshKind.html).

### CDLA-Permissive-2.0

- **Date**: 2026-06-21
- **Owner**: Registry Quality Maintenance
- **Crates**: `webpki-root-certs`, `webpki-roots`
- **License**: CDLA-Permissive-2.0
- **Decision**: Allowed
- **Reason**: These crates ship Mozilla/webpki root certificate data as a data bundle. The CDLA-Permissive-2.0 license is a data-specific permissive license that permits unrestricted use, modification, and distribution. The data is not executable code and does not impose copyleft or network-use obligations. It is pulled transitively by `rustls` / `tauri` networking stacks.
- **Review cadence**: Re-evaluate on each monthly dependency audit or when the root-cert data crate license changes.

## Windows artifact parsers — 2026-11-13

Owner: Parser Maintenance. The application uses released, read-only parser libraries for
Windows shell artifacts and ESE metadata. Versions are pinned in the workspace manifest.

| Package | Version | Application use | License |
|---|---|---|---|
| trash-core | 0.3.2 | Windows Recycle Bin index records, Windows feature only | Apache-2.0 |
| sootmark-shell | 0.3.0 | Shell Link, AutomaticDestinations and CustomDestinations | MIT OR Apache-2.0 |
| sootmark-imagecache | 0.1.0 | Windows thumbnail cache entries and checksum diagnostics | MIT OR Apache-2.0 |
| sootmark-ese | 0.1.1 | Typed ESE header metadata for SRU | MIT OR Apache-2.0 |

The adapters preserve evidence file identity and FILETIME precision. They do not execute
Shell Link targets or invoke ESE recovery. Recycle Bin versions 1/2, thumbnail cache
versions 20/21/30/31/32, two Shell Link variants and both Jump List containers have
committed public fixtures. Fixture origins, redistribution licenses, SHA-256 values and
independent expected fields are in
`testdata/fixtures/public-small/windows-parsers/README.md` and `expected.json`.
The SRU scope remains database-header metadata.

Published sources: [trash-core](https://github.com/SecurityRonin/trash-forensic),
[sootmark-shell](https://github.com/Sootmark/shell),
[sootmark-imagecache](https://github.com/Sootmark/imagecache),
[sootmark-ese](https://github.com/Sootmark/ese).
Their minimum Rust versions are 1.96, 1.80, 1.80 and 1.80 respectively; the workspace
toolchain is 1.99.0. The existing dependency license policy covers these licenses.

## Advisory review — 2026-11-19 (chrono family unmaintained)

Owner: Dependency Maintenance. RustSec issued RUSTSEC-2026-0543 (`chrono`) and
RUSTSEC-2026-0555 (`chrono-tz`) on 2026-11-17. Both are INFO/unmaintained advisories,
cover all versions, and have no patched release.

`chrono 0.4.45` is the workspace-wide timestamp type used by domain, transport,
artifacts, analysis, import, and Ceph code. `chrono-tz 0.10.4` is used only by
analysis-core Linux timezone resolution. Disposition: time-bounded exceptions in
`deny.toml` through 2027-06-30; both crates are maintenance-only findings with no
known exploitable vulnerability. Replacement is deferred to one coordinated
chrono-to-jiff/time migration so the timestamp representation and IANA timezone
database change together; no safe upgrade exists today. New direct `chrono` usage
must be reviewed against that migration rather than extending this exception.

Validation: `cargo tree --locked --target x86_64-pc-windows-msvc -i chrono`,
`cargo tree --locked --target x86_64-pc-windows-msvc -i chrono-tz`,
`cargo deny check advisories bans licenses sources`, and
`scripts/check-deny-exceptions.ps1`.

## Advisory review — 2026-11-16

Owner: Dependency Maintenance. This review resolves the expired 2026-10-16 exceptions by
updating the upstream dependency path instead of renewing them.

### Resolved by tauri-utils refresh

`tauri-utils` was updated from 2.9.3 to 2.10.3 (compatible with the retained `tauri`
2.11.3 requirement `^2.9.3`). The new release requires `urlpattern` `^0.6`; the lockfile now
resolves `urlpattern` 0.6.0, which replaced the `unic-*` family with `icu_properties`.
Cargo.lock no longer contains any `unic-*` package, so RUSTSEC-2025-0075, RUSTSEC-2025-0080,
RUSTSEC-2025-0081, RUSTSEC-2025-0098 and RUSTSEC-2025-0100 no longer match the graph and
their `deny.toml` exceptions were deleted (`unused-ignored-advisory = "deny"` enforces this).

### Remaining unmaintained dependency

| Advisory | Resolved dependency | Exposure and disposition |
|---|---|---|
| [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) | proc-macro-error 1.0.4 | Build-time only via glib-macros 0.18.5 / gtk3-macros 0.18.2 on the Linux GTK path of tauri 2.11.3. `cargo tree --locked --target x86_64-pc-windows-msvc -i proc-macro-error` prints nothing; Windows is the only shipped target. |

This is the second Low-severity renewal and therefore an escalation, not a routine extension:
gtk 0.18 has no release that removes `proc-macro-error`, and replacing it requires an
upstream GTK/Tauri refresh or a workspace patch of the GTK macro stack. The exception expires
on **2026-12-16**. If the dependency path still exists at that date, the release gate stays
red until the GTK/Tauri stack is upgraded or the macro crates are patched locally.

Validation: `cargo update -p tauri-utils --precise 2.10.3`,
`cargo tree --locked --target x86_64-unknown-linux-gnu -i proc-macro-error`,
`cargo tree --locked --target x86_64-pc-windows-msvc -i proc-macro-error`,
`cargo deny check advisories bans licenses sources`, and `scripts/check-deny-exceptions.ps1`.

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

## Evidence decoding providers reviewed on 2026-11-25

| Dependency | License | Enabled surface | Evidence-processing use |
|---|---|---|---|
| `ssh-key = 0.6.7` | MIT OR Apache-2.0 | `std`, `ecdsa` | RustCrypto OpenSSH key, authorized_keys and known_hosts decoding. Public fingerprints, comments and configuration options are retained in Linux artifacts. |
| `compcol = 0.8.0` | MIT | `std`, `lznt1` | LZNT1 decoding on non-Windows hosts through its streaming `DecoderReader`, with a caller-enforced compression-unit output bound. |
| Existing `windows-sys = 0.61.2` | MIT OR Apache-2.0 | `Wdk_Storage_FileSystem` | Windows LZNT1 and XPRESS Huffman decompression through ntdll. Workspace size is requested from the provider, and the output length is checked. |

The provider versions and features are declared in workspace dependencies. The
Windows NTFS reader uses the native LZNT1 provider. A test decodes the same input
with the portable provider and compares every output byte, then checks output
overflow and truncated-input rejection. Complete reads, range reads and
seekable NTFS streams share the compression-unit mapping.

The review used the tagged `RustCrypto/SSH` `ssh-key/v0.6.7` Cargo manifest and
key/configuration APIs, the published `compcol 0.8.0` Cargo manifest and LZNT1
decoder implementation, and the Microsoft `RtlDecompressBuffer`,
`RtlDecompressBufferEx` and `RtlGetCompressionWorkSpaceSize` specifications.

## Storage profile admission reviewed on 2026-11-28

`rust-fs-btrfs = 0.10.2` remains the existing MIT native provider.
The reviewed surface comprises `Filesystem::mount_pool`, checked superblock
selection, chunk profiles/mappings and checksum-verified file reads.
`rust-fs-core = 0.3.14` supplies the existing read-only `BlockRead` interface.
Dependency aliases and versions are centralized in workspace dependencies.
No new native-format decoder or runtime dependency is introduced.

The application admits complete Btrfs device sets only, with SINGLE, DUP, RAID0,
RAID1 and RAID10 bounded by their public native-content oracles. Dataset creation
uses Linux Btrfs and mdadm tooling; the Rust runtime only reads the results.
Members bind FSID, devid, device UUID, generation, geometry, fixed source revisions
and native mapping digest. Unsupported profiles and incomplete sets fail closed.

The separate ZFS candidate remains `lamzfs 0.1.4`, inspected at
`1e0364887921416c2734d9c86fca57cd115fdf17`. Its Cargo manifest declares MIT;
LICENSE-MIT and NOTICE describe the MIT election for the vendored rzfs decoders.
The reviewed APIs are read-only pool import, dataset namespace and file reads.
The README and OpenZFS fixture manifest constrain its native layouts and features.
It is not added to this workspace: project-level pool lifecycle, source attribution,
range authorization and complete native-content acceptance remain required before
ZFS capability admission.
