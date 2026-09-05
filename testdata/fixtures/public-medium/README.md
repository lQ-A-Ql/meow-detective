# Public-Medium Fixtures

`testdata/fixtures/public-medium/` contains medium-sized test data that is checked into the repository for CI-compatible regression testing. These fixtures are larger than `public-small` (tiny synthetic fixtures) but still small enough to be committed to version control (typically <10 MB per file, total directory <50 MB).

## Purpose

- Parser boundary regression testing
- Cross-module integration testing
- Manual verification of artifact extraction
- Consistent baseline for CI test suites

## Directory Layout

| Directory | Content | Typical Artifact Types |
|-----------|---------|----------------------|
| `e01/` | Synthetic E01 forensic images | Disk image parsing, evidence reader integration |
| `iso/` | Deterministic ISO9660/Joliet image | Optical image reader, Joliet names, bounded extents |
| `vmdk/` | Deterministic monolithic-flat VMDK + FLAT extent | Descriptor parsing, logical geometry, ISO composition |
| `ntfs/` | NTFS filesystem samples ($MFT extracts, etc.) | MFT parsing, file record extraction, INDX parsing |
| `prefetch/` | Windows Prefetch files (.pf) | Prefetch parser regression, execution timeline |
| `lnk/` | Windows LNK shortcut files | LNK parsing, shell item extraction |
| `registry/` | Registry hive samples | Registry parsing, key/value enumeration |
| `recycle-bin/` | Recycle Bin $I/$R files | Recycle Bin artifact extraction |

## Requirements for Adding Fixtures

1. **Source documentation**: Each subdirectory must contain a `README.md` or metadata file describing the source and providence of the samples.
2. **Public domain or permissive**: All fixtures in this directory must be suitable for public distribution. No copyrighted or personally identifiable data.
3. **Field commitment**: Where applicable, document the expected field values and counts that tests can assert against.
4. **Size limit**: Individual files should be under 10 MB wherever possible. If a file must be larger, document the justification.
5. **Naming convention**: Use descriptive filenames with the appropriate extension (e.g., `sample-win10-2024.pf`, `example.LNK`).

## Alignment Baselines

When adding new fixtures, provide an alignment baseline table so parsers can be validated:

```json
{
  "fixture": "example.pf",
  "expected": {
    "executable_name": "EXAMPLE.EXE",
    "run_count": 8,
    "last_run_time": "2024-01-15T10:30:00Z",
    "file_size": 47632
  }
}
```

## Current Status

- Directory structure established
- ISO9660/Joliet and monolithic-flat VMDK adapter fixtures are committed
- Remaining individual medium fixtures continue to be populated per implementation roadmap
- See `docs/parser-support-matrix.md` for per-parser fixture requirements

## Relationship to Other Fixture Tiers

| Tier | Directory | Git Tracked | CI Usage | Size Range |
|------|-----------|-------------|----------|------------|
| Public Small | `testdata/fixtures/public-small/` | Yes | Always | <100 KB per file |
| Public Medium | `testdata/fixtures/public-medium/` | Yes (LFS if needed) | Always | 100 KB-10 MB per file |
| Private Real | `testdata/fixtures/private-real-regression/` | No (gitignored) | Never | >1 GB |
