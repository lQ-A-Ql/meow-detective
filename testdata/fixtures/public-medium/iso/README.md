# Public-medium ISO9660/Joliet fixture

- Chain: ISO9660/Joliet
- Visibility: public-medium
- Source: deterministic synthetic generator (`scripts/generate-image-fixtures.ps1`)
- Legal: repository-owned, non-sensitive fixture data
- SHA-256: `dec400ceb2ff73e2f51c75323e14310c3bb8b7ea94315975eac644835d3e95a1`
- Size: 524288 bytes
- Expected JSON: `expected.json`
- Coverage:
  - Primary Volume Descriptor and terminator
  - Joliet Supplementary Volume Descriptor preference
  - root, nested, and binary file directory records
  - UTF-16BE Joliet name decoding
  - bounded file extents and seekable reads
- Notes:
  - Rock Ridge, UDF, multi-extent, interleaved extents, and extended attributes are
    intentionally absent because they are outside the adapter contract.

Regenerate with:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/generate-image-fixtures.ps1
```
