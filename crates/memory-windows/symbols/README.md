# Offline BitLocker memory-recovery symbol tables

Each JSON here is a whitelist extraction from one official Microsoft PDB,
downloaded from the public symbol server:

```text
https://msdl.microsoft.com/download/symbols/<module>.pdb/<GUID><age>/<module>.pdb
```

File naming: `<module>/<PDB-GUID>-<PDB-age>.json`. Lookup keys on the CodeView
(RSDS) GUID; the recorded `pdbAge` is informational only — the RSDS age in the
binary and the PDB info-stream age can differ (this is normal; the GUID is the
authoritative identity).

## Contents

- `ntkrnlmp/953A8DE8-...-6.json` — public PDB **with full type info**.
  Globals (`ObpRootDirectoryObject`, `ObpInfoMaskToOffset`) and struct layouts
  (`_OBJECT_DIRECTORY*`, `_OBJECT_HEADER`, `_UNICODE_STRING`, `_DRIVER_OBJECT`,
  `_DEVICE_OBJECT`, `_KLDR_DATA_TABLE_ENTRY`, …). All extracted offsets were
  cross-validated against the reviewed Windows 11 26100 runtime profile and
  match exactly.
- `fvevol/47808A31-...-3.json` — public PDB **stripped of type info** (0 type
  records; this asymmetry is expected for driver PDBs). Contains 60
  BitLocker-relevant public symbols (58 `FVE_KEYRING_*` request GUIDs plus
  `GetKeyRingFromKsr`, `ReleaseKeyRing`, `FvepComputeKeyFromPassphrase`,
  `FvepVmkInfoProcessStretchKey`, `IoctlFveProvideVmk`) with RVAs. Struct
  offsets for the fvevol keyring/client/volume-context layouts cannot be
  extracted from stripped PDBs and must stay manually reviewed per build;
  the function RVAs here are the reverse-engineering anchors.

## Regenerating / adding a build

1. Download the module PDB for the target GUID+age from the symbol server.
2. Run the extractor (workspace-detached tool):

   ```bash
   cd scripts/dev/pdb-symbol-extract
   cargo run --release --bin pdb-symbol-extract -- \
     configs/<module>.json <input.pdb> <output.json>
   ```

3. Cross-check the extracted offsets against the runtime profile for that
   build before wiring the JSON into the resolver. Unknown builds must keep
   failing closed with the typed `UnsupportedBitLockerMemoryProfile` error;
   never derive offsets by guessing.

Only extracted facts (RVAs, field offsets) are stored. PDB files themselves
must not be committed.
