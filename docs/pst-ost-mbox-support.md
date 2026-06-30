# PST/OST/MBOX Email Container Support

This document describes the Forensics Workbench support for Microsoft Outlook
PST/OST containers and UNIX-style mbox mailboxes.

## Supported Formats

| Format | Extension | Status | Notes |
|--------|-----------|--------|-------|
| PST (Personal Storage Table) | `.pst` | Supported | Unicode 64-bit PST files |
| OST (Offline Storage Table) | `.ost` | Supported | Detected by extension; binary layout same as PST |
| MBOX (mailbox folder) | `.mbox` | Supported | Thunderbird-style `mboxrd` escaping |

## Architecture

Email containers are discovered and extracted by the analysis pipeline:

1. **Candidate discovery** – files whose names end in `.pst`, `.ost`, or `.mbox`
   are flagged as email evidence candidates.
2. **Magic validation** – PST/OST candidates are checked for the `!BDN` magic
   bytes at offset 0.
3. **Extraction** – `extract_email_candidate` dispatches to the appropriate
   parser and produces `Artifact` rows plus `TimelineEvent` rows.

```text
EvidenceCandidate
    ↓
extract_email_candidate
    ├── extract_eml_candidate     (single RFC 5322 message)
    ├── extract_mbox_candidate    (mbox folder)
    └── extract_pst_candidate     (PST/OST temp file → PstReader/OstReader)
```

## PST/OST Details

The `containers-pst` crate implements a minimal read-only Unicode PST/OST
reader. It parses:

- Header (magic, version, Unicode flag, root BREFs)
- Block BTree (BBT) and Node BTree (NBT)
- Heap-on-Node property contexts
- Message class filtering (`IPM.Note` variants)
- Folder path construction from display-name properties

OST files share the same binary format as PST files. The extractor determines
file kind primarily by extension, falling back to header heuristics if needed.

### Synthetic Fixtures

The `containers-pst/examples/generate_medium_fixture.rs` example builds
multi-message synthetic PST/OST files for regression testing. The synthetic
builder writes:

- Header with Unicode flag
- BBT and NBT leaf pages
- Property contexts for the message store, root folder, and each message
- Inline UTF-16LE string properties using full 4-byte property tags

## MBOX Details

The mbox extractor:

- Supports `mboxrd` `>From ` escaping.
- Splits messages on `From ` lines that start a new entry.
- Parses each message with the same EML parser used for `.eml` files.
- Produces one artifact per message with `containerPath` set to the mbox file
  name.

## Frontend Presentation

Email artifacts are rendered in the **Email Extraction** panel:

- Table columns: From, Subject, Date, Folder (container path).
- Detail card shows: headers, body preview, attachments, container path, and
  message class for PST/OST items.

## Limitations

- ANSI PST files are not supported.
- Encrypted or password-protected PST/OST files are not supported.
- Very large PST/OST files are currently loaded entirely into memory during
  extraction; a streaming/block-cached reader is planned for a future release.
- Full sub-node BTree and attachment table parsing for PST/OST is partial;
  attachments may not be extracted from complex real-world PST files.

## Fixtures

| Fixture | Location | Contents |
|---------|----------|----------|
| public-small | `testdata/fixtures/public-small/email/` | 4 EML/EMLX + 3 mbox + 1 PST + 1 OST |
| public-medium | `testdata/fixtures/public-medium/email/` | 13 EML + 55-message mbox + 10-message PST/OST |

## Real-World Regression

Real `.eml`, `.mbox`, `.pst`, and `.ost` samples can be validated with the
ignored integration tests in:

- `crates/containers-pst/tests/email_real_regression_test.rs`

Set `FORENSICS_EMAIL_FIXTURE_DIR` and run:

```powershell
$env:FORENSICS_EMAIL_FIXTURE_DIR = "C:\\path\\to\\email-samples"
cargo test -p containers-pst --test email_real_regression_test -- --ignored --nocapture
```

## Performance Baseline

See `docs/benchmark-results/2026-06-21-email-extraction-bench.md` for
measured throughput on synthetic fixtures:

- 1 MiB mbox parses in ~0.08 s (>12 MiB/s).
- 10-message synthetic PST parses in ~5 ms.

## References

- `[MS-PST]`: Outlook Personal Folders (.pst) File Format
- RFC 4155: The application/mbox Media Type
- RFC 5322: Internet Message Format
