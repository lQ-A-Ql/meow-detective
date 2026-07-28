# Four-sample E01 file extraction benchmark

- Date: 2026-07-29 00:22:58 +08:00
- Commit: `53a48b35ed69`
- Build profile: Rust `release`
- Host: AMD Ryzen 7 8745HS w/ Radeon 780M Graphics, 8 cores / 16 logical processors, 31.29 GiB RAM
- OS: Microsoft Windows 11 专业版 10.0.26200 build 26200
- Storage: C: NTFS on TOPMORE Leo Q (NVMe); D: NTFS on TR SS4P1024-R5 (NVMe); E: exFAT on HP SSD EX900 500GB (NVMe)
- Mode: post-enumeration warm extraction, one successful 128-512 MiB file per sample
- Destination: system temporary directory; SHA-256, flush, sync, and atomic publish are included
- Scheduling: samples and exports are serial; import/enumeration time is excluded from the reported extraction phases

| Sample | Image | Image GiB | Internal file | Partition | BitLocker | File MiB | Prepare s | Copy s | Finalize s | Total s | Copy MiB/s | Total MiB/s | Progress events |
|---|---|---:|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Windows 1 | liuyang_pc.E01 | 29.169 | `BaiduNetdiskDownload/价值4万的「优化版」数字资产交易所源码｜币币交易｜C2C交易｜交易机器人｜撮合交易｜合约交易.zip` | 5 | yes | 203.029 | 0.470 | 0.182 | 0.120 | 0.773 | 1110.261 | 262.372 | 28 |
| Windows 2 | 检材2.E01 | 17.827 | `Program Files/Google/Chrome/Application/126.0.6478.183/chrome.dll` | 1 | no | 220.153 | 0.833 | 0.309 | 0.130 | 1.272 | 711.611 | 172.946 | 30 |
| Linux 1 | 检材3.E01 | 2.580 | `usr/sbin/mysqld` | 2 | no | 244.140 | 0.760 | 0.200 | 0.150 | 1.111 | 1218.335 | 219.563 | 33 |
| Linux 2 | PC.E01 | 9.436 | `usr/share/burpsuite/burpsuite.jar` | 0 | no | 311.555 | 0.437 | 0.268 | 0.260 | 0.966 | 1158.525 | 322.270 | 41 |

## Findings

- Warm-cache copy throughput spans 711.611-1218.335 MiB/s across the four source and filesystem combinations.
- End-to-end throughput is 172.946-322.270 MiB/s; reader/filesystem preparation takes 0.437-0.833 s and durable finalization takes 0.120-0.260 s per extraction.
- Copy throughput can exceed sustained physical-device throughput because enumeration and candidate preview warm the Windows page cache. These values must not be treated as cold-cache or long-duration sequential-I/O limits.
- Windows 1 exercises a BitLocker-backed file. Windows 2 falls back to a non-BitLocker file because its unlocked BitLocker catalog has no regular file in the 128-512 MiB benchmark tier.

## Interpretation boundary

This is a single-run, post-enumeration warm-cache private-sample baseline, not a release threshold. It measures the production evidence reader, SHA-256 calculation, destination write, durable sync, and atomic publish. It does not include image import or filesystem enumeration. Cold-cache runs, repeated-run p50/p95, sustained multi-GiB exports, and RSS/CPU telemetry remain separate follow-up work.

Raw cargo output: `artifacts/file-extraction-benchmark/cargo-test.log`
