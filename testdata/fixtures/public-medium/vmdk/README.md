# Public-medium monolithic-flat VMDK fixture

- Chain: monolithic-flat VMDK
- Visibility: public-medium
- Source: deterministic synthetic generator (`scripts/generate-image-fixtures.ps1`)
- Legal: repository-owned, non-sensitive fixture data
- Descriptor SHA-256: `d7add8fcd50eccb76675c12c13d4298a82e2a7a32148a40f6a89ee05790fe3de`
- FLAT extent SHA-256: `dec400ceb2ff73e2f51c75323e14310c3bb8b7ea94315975eac644835d3e95a1`
- Logical size: 524288 bytes (`1024` sectors at 512 bytes)
- Expected JSON: `expected.json`
- Coverage:
  - UTF-8 `createType="monolithicFlat"` descriptor
  - one zero-offset `FLAT` extent
  - descriptor plus extent backing manifest
  - ISO9660/Joliet composition over the logical VMDK bytes
- Notes:
  - The extent intentionally contains the committed medium ISO fixture. This is
    a composition oracle, not a claim that every VMDK is bootable.

Regenerate with:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/generate-image-fixtures.ps1
```
