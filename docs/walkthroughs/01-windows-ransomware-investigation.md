# Walkthrough: Windows Ransomware Investigation

This walkthrough follows the 獬豸杯 case (检材2.E01) — a single-disk ransomware investigation using Forensics Workbench V3. The suspect workstation is a Windows 10/11 system with three NTFS partitions. We will mount the E01 image, extract evidence, run cross-source correlation, and export findings.

## 0. Prerequisites

- Forensics Workbench V3 installed
- Case file: `检材2.E01` (single-disk, E01 format, ~256 GB)
- Loaded rule packs: `windows-user-activity`, `ransomware-indicators`
- Verification config at `<case_root>/verification.toml` with `required_packs = ["windows-user-activity", "ransomware-indicators"]`

---

## 1. Open Case and Mount the Image

1. Launch the application. Create a new case named `獬豸杯-检材2`.
2. Set case directory to `D:\cases\xiezhi-cup\`.
3. Go to **Import > E01 Image**. Select `检材2.E01`.
4. Enable the **Probe First** toggle. The probe phase runs before full import to validate image integrity and auto-detect partition layout.

**Expected probe output (E01 metadata):**

```
Probe: 检材2.E01
  Format: EnCase Evidence File (E01)
  Disk size: 256,060,514,304 bytes (256 GB)
  Sector size: 512
  Segments: 1 (no split)
  GUID: 4f9a2b3c1d5e67890a1b2c3d4e5f6789
  Partitions detected: 3

  Partition 0: NTFS (System Reserved) — 104,857,600 bytes (100 MiB), offset 1,048,576
  Partition 1: NTFS (System) — 255,550,554,112 bytes (238 GiB), offset 105,906,176
  Partition 2: NTFS (Recovery) —  404,750,336 bytes (386 MiB), offset 255,656,460,288
```

5. Confirm the auto-detected partition layout. The probe result shows 3 NTFS partitions — standard for a Windows install.
6. Proceed to import with **full artifact extraction**, **timeline**, and **correlation** enabled.

---

## 2. Import the MFT and Browse the File Tree

After image mount, the catalog phase walks the NTFS $MFT on the system partition (partition 1).

1. Wait for catalog to complete. Monitor progress in the **Import Progress** panel.
2. When catalog finishes, the V3 dashboard updates:

```
File nodes: 69,427
Directory nodes: 17,312
Contains edges: 86,739
```

3. Open the **File Browser**. The tree now reflects the entire NTFS hierarchy.
4. Navigate to `C:\Users\<suspect>\Desktop\`. Look for suspicious files: ransom note text files, encrypted file extensions, or unknown executables.
5. Navigate to `C:\Users\<suspect>\AppData\Local\Temp\` — ransomware payloads commonly stage in Temp.
6. Navigate to `C:\Windows\System32\config\` — Registry hive files are here and will feed the Registry parser.

---

## 3. Run Artifact Extraction

After catalog completes, the artifact extraction phase processes parsers in parallel.

**Command invoked (internal, shown as Tauri commands):**

```
extract_artifacts(case_id, families: ["Registry", "EVTX", "Prefetch", "LNK", "JumpList", "RecycleBin", "BrowserHistory", "BrowserDownloads"])
```

**Expected output — Artifact node counts on the V3 dashboard:**

```
Artifact nodes: 18,452
  Registry      : 4,211
  EVTX          : 3,087
  Prefetch      :   342
  LNK           : 1,203
  JumpList      :   215
  RecycleBin    :    89
  BrowserHistory: 8,120
  BrowserDownloads: 1,185
```

Key artifacts for ransomware investigation:

1. **Registry hives** — Check `HKLM\Software\Microsoft\Windows\CurrentVersion\Run` for persistence entries. Look for newly-added entries with unusual executable paths in `AppData\Local\Temp\` or `ProgramData\`.
2. **EVTX logs** — Filter Event Log events by EventID 4688 (process creation). Cross-reference the process name with the Prefetch executable list.
3. **Prefetch** — Sort by `last_run_time` descending. Entries in Temp directories or with randomly-generated names (e.g., `XYZWER.EXE-C3B9F2A1.pf`) are high-signal. Run count > 1 from a Temp path is suspicious.
4. **Browser downloads** — Filter downloads within 48 hours of the earliest ransomware indicator timestamp. Download source URLs pointing to file-sharing or phishing domains are strong leads.

---

## 4. Correlate Leads Across Sources

Run the correlation engine with both loaded rule packs.

1. Go to **Correlation Workspace**. Select packs `windows-user-activity` and `ransomware-indicators`.
2. Click **Run Correlation**.

**Expected correlation output (partial):**

```
Correlation complete: 347 leads generated
  Strong   : 142
  Moderate : 158
  Weak     :  47

Rule pack contributions:
  windows-user-activity:   281 leads
  ransomware-indicators:    66 leads
```

**How to read confidence levels in V3:**

| Confidence | Meaning | Example |
|-----------|---------|---------|
| **Strong** | Exact path match OR multiple independent signals converge | Prefetch executable path `==` File Browser path AND Run key value data `contains` that path |
| **Moderate** | Partial match (name only, or path stem) | Prefetch `executable_name` matches a file in `AppData\Local\Temp\` but full path cannot be verified |
| **Weak** | Temporal proximity only (within 24h window) | Browser download timestamp is within 24h of a Prefetch run timestamp for a different executable |

**Key correlation leads to inspect:**

- **Registry-to-File leads**: Run key value data pointing to `AppData\Local\Temp\ransomware.exe` → `CorrelatesWith` edge to File node at that path. Confidence: **Strong** (exact path match).
- **Prefetch-to-File leads**: Prefetch `PF_SUSPICIOUS.EXE-XXXXXXXX.pf` → `CorrelatesWith` edge to File node `C:\Users\<suspect>\Downloads\suspicious.exe`. Confidence: **Strong** (executable name + directory match).
- **BrowserDownload-to-Timeline leads**: Download of `.exe` at 2026-06-14 02:13 UTC → `TemporalContext` edge to TimelineEvent for file creation at 2026-06-14 02:14 UTC. Confidence: **Moderate** (sub-1-minute temporal proximity, name similarity).
- **LNK-to-RecycleBin leads**: LNK file target path matches a RecycleBin `original_path`. Confidence: **Strong** (exact path match; deleted shortcut).

Use the **Graph Query** to trace the attacker's chain:

```json
{
  "startNode": { "nodeId": "file:Users/suspect/AppData/Local/Temp/encryptor.exe" },
  "edgeFilters": ["CorrelatesWith", "References", "Precedes"],
  "maxDepth": 4,
  "confidenceFloor": "Moderate"
}
```

This traversal reveals: the Temp executable → linked Registry persistence key → linked Prefetch record → temporally-preceding Browser download → source URL.

---

## 5. Document Findings in the Case Notebook

1. Open the **Notebook Panel** from the sidebar.
2. Create a new **Finding** entry titled `Ransomware Payload Chain`.
3. Write the narrative, then use the **Citation Picker** to cite each piece of evidence:

```markdown
# Finding: Ransomware Payload Delivery and Execution Chain

The ransomware payload `encryptor.exe` was delivered via a browser download
from `hxxps://evil-cdn[.]example/payload/update.exe` and executed from
`C:\Users\suspect\AppData\Local\Temp\encryptor.exe`.

## Delivery Evidence
- [BrowserDownload: update.exe from evil-cdn.example — 2026-06-14 02:13 UTC]
- [File: C:\Users\suspect\AppData\Local\Temp\encryptor.exe — created 2026-06-14 02:14 UTC]

## Execution Evidence
- [Prefetch: ENCRYPTOR.EXE-A1B2C3D4.pf — 7 runs, first run 2026-06-14 02:14 UTC]
- [EVTX EventID 4688: encryptor.exe process creation — 2026-06-14 02:14 UTC]

## Persistence Evidence
- [Registry: HKLM\Software\Microsoft\Windows\CurrentVersion\Run\EncryptorService]
- [Correlation Lead: Prefetch-Temp-to-Registry-Run (Strong)]

## Encryption Activity
- [TimelineEvent: 15,427 file modify events within 300 seconds starting 02:14 UTC]
- Ransom note: [File: C:\Users\suspect\Desktop\README_DECRYPT.txt]

Conclusion: Confirmed ransomware incident. Payload delivered via browser download,
executed from Temp directory, established registry persistence, and encrypted
user files within a 5-minute window before self-deleting.
```

4. Mark the entry status as **Reviewed**. Create child entries for follow-up actions.

---

## 6. Export the Report

1. Go to **Reports > New Report**. Choose **HTML** format.
2. Include:
   - Evidence Graph Summary
   - Platform Coverage (Windows artifact families)
   - Correlation Leads (all **Strong** and **Moderate**)
   - Case Notebook (all **Reviewed** and **Final** entries)
   - Investigation Timeline (step replay log)
   - Rule Pack Coverage
3. Generate the report. Verify the export includes clickable notebook citations.

**Report sections in the output:**

```
獬豸杯-检材2 — Investigation Report
├── 1. Evidence Graph Summary (86K+ nodes, 104K+ edges)
├── 2. Correlation Leads (347 leads, 142 Strong)
├── 3. Case Notebook (1 Finding, 3 child Action Items)
├── 4. Investigation Steps (11 steps, 14m 32s total)
└── 5. Rule Pack Coverage (2 packs, 100% rules executed)
```

---

## Quick Reference: Ransomware Investigation Flow

```
Mount E01 → Probe (3 NTFS partitions)
      │
      ▼
Import MFT → File Browser (69K files)
      │
      ▼
Extract Artifacts (Registry, EVTX, Prefetch, LNK, Browser)
      │
      ▼
Correlate (cross-source leads: Registry→File, Prefetch→File, Browser→Timeline)
      │
      ▼
Graph Query → Trace attack chain (delivery → execution → persistence → encryption)
      │
      ▼
Notebook → Document findings with evidence citations
      │
      ▼
Export → HTML report with correlation details, citations, and step replay
```

---

*Walkthrough 01-Windows-Ransomware version 1.0. Last updated: 2026-06-15.*
