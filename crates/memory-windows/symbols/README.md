# Offline BitLocker memory-recovery symbol tables

Each JSON here is a whitelist extraction from one official Microsoft PDB,
downloaded from the public symbol server:

```text
https://msdl.microsoft.com/download/symbols/<module>.pdb/<GUID><age>/<module>.pdb
```

File naming: `<build>-<PDB-GUID>.json` (e.g.
`26100.1742-953A8DE8-80B0-818C-32DA-2DEC1D79C2D9.json`). Registry lookup keys
on the CodeView (RSDS) GUID; the recorded `pdbAge` is informational only —
the RSDS age in the binary and the PDB info-stream age can differ (this is
normal; the GUID is the authoritative identity).

## Collection

- `ntkrnlmp/` — 1077 per-build tables covering Windows 10 10240 through
  Windows 11 28000, harvested from the winbindex metadata (PE
  timestamp+size), each binary's RSDS record, and the matching Microsoft PDB
  (~96% of indexed builds; the remainder is absent from the public symbol
  server — either the binary itself or only its PDB). Each table carries the
  two object-manager globals (`ObpRootDirectoryObject`,
  `ObpInfoMaskToOffset`) and the object/driver/device/module layouts for that
  build. The registry include is generated at
  `src/keyring_recovery/symbol_registry_generated.rs`.
- `fvevol/47808A31-...-3.json` — public PDB **stripped of type info** (driver
  PDBs carry no type records). Holds 60 BitLocker-relevant public symbols
  with RVAs (`FVE_KEYRING_*` request GUIDs, keyring/VMK functions) as
  reverse-engineering anchors; fvevol struct offsets stay out of the profile
  system entirely (signature-anchored scans cover them).

## Layout stability note

Every field offset the recovery path consumes (`_DEVICE_OBJECT`,
`_DRIVER_OBJECT`, `_DRIVER_EXTENSION`, `_OBJECT_HEADER`,
`_OBJECT_HEADER_NAME_INFO`, `_OBJECT_DIRECTORY(_ENTRY)`, `_UNICODE_STRING`,
`_KLDR_DATA_TABLE_ENTRY`) was verified invariant across all extracted
profiles, so those live in source as reviewed constants. Per-build data is
only the two object-manager global RVAs, used by the object-directory fast
path; unknown builds proceed through the version-free driver-object carve.

## Regenerating / adding a build

1. Download the module PDB for the target GUID+age from the symbol server.
2. Run the extractor (workspace-detached tool):

   ```bash
   cd scripts/dev/pdb-symbol-extract
   cargo run --release --bin pdb-symbol-extract -- \
     configs/<module>.json <input.pdb> <output.json>
   ```

3. Place the JSON as `<build>-<guid>.json` under `ntkrnlmp/` and regenerate
   `symbol_registry_generated.rs` from the collection. Unknown builds keep
   working through the version-free fallback regardless.

Only extracted facts (RVAs, field offsets) are stored. PDB files themselves
must not be committed.
