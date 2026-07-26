# Synthetic PST Fixtures

This directory contains deterministic in-memory-only test fixtures for PST/OST
parsing. No binary PST files are committed to the repository; tests build
minimal synthetic NDB/LTP structures in memory using the `build_synthetic_pst()`
helper in `crates/containers-pst/src/pst.rs`.

## Fixture coverage

| Test helper                     | Coverage                                      |
|---------------------------------|-----------------------------------------------|
| `build_synthetic_unicode_pst()` | 4 KiB Unicode PST: header, NBT, BBT, 3 HN/PC pages |

## Synthetic PST layout

The builder produces a 4096-byte (8-page) Unicode PST:

| Page | Offset  | Content                                              |
|------|---------|------------------------------------------------------|
| 0    | 0       | Header: "!BDN" magic, wVer=23, ROOT with NBT/BBT refs|
| 1    | 512     | (unused padding)                                     |
| 2    | 1024    | BBT leaf page: 6 block entries mapping BID → offset  |
| 3    | 1536    | (unused padding)                                     |
| 4    | 2048    | NBT root leaf page: 4 NID entries                     |
| 5    | 2560    | Property context: message store (DisplayName)         |
| 6    | 3072    | Property context: root folder (DisplayName)           |
| 7    | 3584    | Property context: synthetic message (Subject, Class,  |
|      |         | SenderName, SenderEmail)                             |

## Why no binary fixtures on disk?

- The NDB/LTP binary format is well-documented (Microsoft Open Specification
  [MS-PST]).
- In-memory construction avoids binary blobs in version control and allows
  tests to exercise precise edge cases (missing fields, invalid BREFs, etc.)
  by mutating the byte buffer.
- A real-world PST regression test point will be added when a sanitized
  sample becomes available.

## Related documentation

- `[MS-PST]`: Outlook Personal Folders File Format (.pst) Structure
- `[MS-OXOSFLD]`: Special Folders Protocol
- `[MS-OXPROPS]`: Exchange Server Protocols Master Property List
