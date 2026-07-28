# Liuyang BitLocker 3 GiB file extraction benchmark

- Date: 2026-07-29 01:58:42 +08:00
- Commit: `76496f59e5e9`
- Build profile: Rust `release`
- Host: AMD Ryzen 7 8745HS w/ Radeon 780M Graphics, 8 cores / 16 logical processors, 31.29 GiB RAM
- OS: Microsoft Windows 11 专业版 10.0.26200 build 26200
- Storage: C: NTFS on TOPMORE Leo Q (NVMe); E: exFAT on HP SSD EX900 500GB (NVMe)
- Mode: post-enumeration 3 GiB BitLocker extraction
- Destination: system temporary directory; SHA-256, flush, sync, and atomic publish are included
- Verification: destination size and an independent post-timing SHA-256 re-read must match the extraction result
- Scheduling: benchmark samples are serial; this export uses two independent bounded NTFS readers, one ordered writer, and sequential SHA-256; import/enumeration time is excluded

| Sample | Image | Image GiB | Internal file | Partition | BitLocker | File MiB | Prepare s | Copy s | Finalize s | Total s | Copy MiB/s | Total MiB/s | Progress events |
|---|---|---:|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Windows 1 / BitLocker 3 GiB | liuyang_pc.E01 | 29.169 | `BaiduNetdiskDownload/WPSOffice_RCTSE_2023_12.1.0.24655_x64_20260115_雨糖科技.exe` | 5 | yes | 3072.000 | 0.004 | 5.004 | 0.566 | 5.576 | 613.812 | 550.890 | 386 |

## Serial comparison

| Implementation | Commit | Copy s | Total s | Total MiB/s | SHA-256 |
|---|---|---:|---:|---:|---|
| Serial 1 MiB stream | `15cc198355f0` | 8.763 | 8.932 | 343.898 | match |
| Bounded parallel readers | `76496f59e5e9` | 5.004 | 5.576 | 550.890 | match |

The committed parallel implementation reduced copy time by `42.9%` and end-to-end time by
`37.6%`; end-to-end throughput increased by `60.2%`. Both runs produced the same digest.

## Memory

| Sample | RSS before MiB | RSS after MiB | Peak RSS before MiB | Peak RSS after MiB |
|---|---:|---:|---:|---:|
| Windows 1 / BitLocker 3 GiB | 874 | 623 | 1384 | 1384 |

The process-wide lifetime peak did not increase during extraction. The extraction window is
bounded to four in-flight `4 MiB` chunks (`16 MiB` of chunk payload), with two independent
readers and one ordered writer. Parallel mode requires a file of at least `512 MiB`, at least
two logical processors, and, when RSS telemetry is available, at least `128 MiB` below the
existing `4 GiB` soft limit. Unsupported or compressed NTFS streams fall back to serial export.

## Integrity verification

| Sample | Bytes | SHA-256 |
|---|---:|---|
| Windows 1 / BitLocker 3 GiB | 3221225472 | `9a87318e9ca5e1b0861f1cc46a10424a668db530b38d14946da87d43a98b5134` |

## Findings

- Copy throughput is 613.812 MiB/s for the measured source and filesystem combination.
- End-to-end throughput is 550.890 MiB/s; reader/filesystem preparation takes 0.004 s and durable finalization takes 0.566 s.
- Copy throughput can exceed sustained physical-device throughput because enumeration and candidate preview warm the Windows page cache. These values must not be treated as cold-cache or long-duration sequential-I/O limits.
- The selected file is exactly 3 GiB and the benchmark rejects non-BitLocker candidates.

## Interpretation boundary

This is a single-run, post-enumeration private-sample baseline, not a release threshold. It measures one complete 3 GiB BitLocker export through the production evidence reader, SHA-256 calculation, destination write, durable sync, and atomic publish. It does not include image import or filesystem enumeration. Windows page-cache warming can materially affect the result. Controlled cold-cache runs, repeated-run p50/p95, and high-frequency extraction-only RSS/CPU sampling remain separate follow-up work; the process-wide RSS counters above cannot isolate short-lived peaks inside the extraction interval.

Raw cargo output: `artifacts/file-extraction-benchmark/cargo-test.log`
