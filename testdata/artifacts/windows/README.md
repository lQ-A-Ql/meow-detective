# Windows Artifact Test Fixtures

## How to collect samples

### Prefetch (.pf)
Location: `C:\Windows\Prefetch\`
- Files are named `EXENAME-HASH.pf`
- Copy 2-3 files, note the filename (it encodes the executable name)
- For each .pf file, create an `expected.json`:
```json
{
  "file": "CMD.EXE-DEADBEEF.pf",
  "expected": {
    "executable": "CMD.EXE",
    "run_count_gt": 0,
    "has_run_times": true
  }
}
```

### LNK (.lnk)
Location: `%APPDATA%\Microsoft\Windows\Recent\` or Desktop shortcuts
- Copy .lnk files, note expected target path
- For each, create `expected.json`:
```json
{
  "file": "cmd.exe.lnk",
  "expected": {
    "has_target_path": true,
    "target_contains": "cmd.exe"
  }
}
```

### Recycle Bin ($I)
Location: `C:\$Recycle.Bin\<SID>\`
- Copy $I files. The test verifies an artifact is produced.
- expected.json format:
```json
[
  { "file": "$IA1B2C3D4.EXE", "expected": {} }
]
```

### Registry Hive
Location: `C:\Windows\System32\config\` (SYSTEM, SOFTWARE) or `%USERPROFILE%\NTUSER.DAT`
- Copy hive files. The test verifies an artifact is produced.
- expected.json format:
```json
[
  { "file": "SYSTEM", "expected": {} }
]
```

## Directory structure

```
testdata/artifacts/windows/
├── README.md               # this file
├── prefetch/
│   ├── expected.json       # test expectations
│   ├── *.pf                 # real Prefetch files
│   └── .gitkeep
├── lnk/
│   ├── expected.json
│   ├── *.lnk
│   └── .gitkeep
├── recycle-bin/
│   ├── expected.json
│   ├── $I*
│   └── .gitkeep
└── registry/
    ├── expected.json
    ├── *.dat
    └── .gitkeep
```

## Running fixture tests

```bash
cargo test -p artifacts-windows -- --ignored
```

Fixture tests are `#[ignore]` by default. Unignore them after placing real files in testdata/.
