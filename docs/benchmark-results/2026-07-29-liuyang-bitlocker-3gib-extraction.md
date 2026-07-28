# Liuyang BitLocker 3 GiB file extraction benchmark

- Date: 2026-07-29 01:07:38 +08:00
- Commit: `15cc198355f0`
- Build profile: Rust `release`
- Host: AMD Ryzen 7 8745HS w/ Radeon 780M Graphics, 8 cores / 16 logical processors, 31.29 GiB RAM
- OS: Microsoft Windows 11 专业版 10.0.26200 build 26200
- Storage: C: NTFS on TOPMORE Leo Q (NVMe); E: exFAT on HP SSD EX900 500GB (NVMe)
- Mode: post-enumeration 3 GiB BitLocker extraction
- Destination: system temporary directory; SHA-256, flush, sync, and atomic publish are included
- Verification: destination size and an independent post-timing SHA-256 re-read must match the extraction result
- Scheduling: samples and exports are serial; import/enumeration time is excluded from the reported extraction phases

| Sample | Image | Image GiB | Internal file | Partition | BitLocker | File MiB | Prepare s | Copy s | Finalize s | Total s | Copy MiB/s | Total MiB/s | Progress events |
|---|---|---:|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Windows 1 / BitLocker 3 GiB | liuyang_pc.E01 | 29.169 | `BaiduNetdiskDownload/WPSOffice_RCTSE_2023_12.1.0.24655_x64_20260115_雨糖科技.exe` | 5 | yes | 3072.000 | 0.005 | 8.763 | 0.164 | 8.932 | 350.543 | 343.898 | 386 |

## Integrity verification

| Sample | Bytes | SHA-256 |
|---|---:|---|
| Windows 1 / BitLocker 3 GiB | 3221225472 | `9a87318e9ca5e1b0861f1cc46a10424a668db530b38d14946da87d43a98b5134` |

## Findings

- Copy throughput spans 350.543-350.543 MiB/s across the measured source and filesystem combination(s).
- End-to-end throughput is 343.898-343.898 MiB/s; reader/filesystem preparation takes 0.005-0.005 s and durable finalization takes 0.164-0.164 s per extraction.
- Copy throughput can exceed sustained physical-device throughput because enumeration and candidate preview warm the Windows page cache. These values must not be treated as cold-cache or long-duration sequential-I/O limits.
- The selected file is exactly 3 GiB and the benchmark rejects non-BitLocker candidates.

## Interpretation boundary

This is a single-run, post-enumeration private-sample baseline, not a release threshold. It measures one complete 3 GiB BitLocker export through the production evidence reader, SHA-256 calculation, destination write, durable sync, and atomic publish. It does not include image import or filesystem enumeration. Controlled cold-cache runs, repeated-run p50/p95, and RSS/CPU telemetry remain separate follow-up work.

Raw cargo output: `artifacts/file-extraction-benchmark/cargo-test.log`
