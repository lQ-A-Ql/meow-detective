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

- `ntkrnlmp/manifest.json` — the slim registry source: 1077 per-build rows
  (Windows 10 10240 through Windows 11 28000, ~96% of indexed builds; the
  remainder is absent from the public symbol server). Each row carries only
  the per-build facts the recovery path needs: build label, PDB GUID/age,
  and the two object-manager global RVAs (`ObpRootDirectoryObject`,
  `ObpInfoMaskToOffset`). The runtime registry is the generated static table
  at `src/keyring_recovery/symbol_registry_generated/` (chunked part files
  under the module-size guard's 500-line target, aggregated by `mod.rs`).
- `fvevol/47808A31-...-3.json` — public PDB **stripped of type info** (driver
  PDBs carry no type records). Holds 60 BitLocker-relevant public symbols
  with RVAs (`FVE_KEYRING_*` request GUIDs, keyring/VMK functions) as
  reverse-engineering anchors; fvevol struct offsets stay out of the profile
  system entirely (signature-anchored scans cover them).

## Layout stability note

Every field offset the recovery path consumes (`_DEVICE_OBJECT`,
`_DRIVER_OBJECT`, `_DRIVER_EXTENSION`, `_OBJECT_HEADER`,
`_OBJECT_HEADER_NAME_INFO`, `_OBJECT_DIRECTORY(_ENTRY)`, `_UNICODE_STRING`,
`_KLDR_DATA_TABLE_ENTRY`) was verified invariant across all 1077 harvested
profiles — only five trailing, never-consumed fields vary
(`_OBJECT_DIRECTORY.{Flags,NamespaceEntry,SessionId,SessionObject}` and
`_FSRTL_ADVANCED_FCB_HEADER.ReservedContext`). Those layouts therefore live
in source as reviewed constants; per-build data is only the two global RVAs
in the manifest, used by the object-directory fast path. Unknown builds
proceed through the version-free driver-object carve.

## Regenerating / adding a build

1. Download the module PDB for the target GUID+age from the symbol server.
2. Run the extractor (workspace-detached tool):

   ```bash
   cd scripts/dev/pdb-symbol-extract
   cargo run --release --bin pdb-symbol-extract -- \
     configs/<module>.json <input.pdb> <output.json>
   ```

3. Place the JSON as `<build>-<guid>.json` under `ntkrnlmp/` and regenerate
   `symbol_registry_generated/` from the collection. Unknown builds keep
   working through the version-free fallback regardless.

Only extracted facts (RVAs, field offsets) are stored. PDB files themselves
must not be committed.
