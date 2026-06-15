# Public-Medium macOS Artifact Fixtures

`testdata/fixtures/public-medium/macos/` contains medium-sized macOS forensic artifacts sourced from a controlled macOS VM snapshot. These fixtures are checked into the repository for CI-compatible regression testing of macOS artifact parsers.

## Purpose

- Validate macOS parser correctness at medium scale (hundreds to thousands of records)
- Exercise edge cases: binary+XML plist variants, unified log chunking, Spotlight index structures
- Provide alignment baselines for automated assertion tests
- Enable cross-module integration testing (timeline, search, file system correlation)

## Acquisition Source

All fixtures are to be sourced from a controlled macOS VM (macOS 13 Ventura or equivalent) that undergoes a scripted activity session:

1. User login and GUI session with standard applications (Safari, Finder, TextEdit, Terminal, System Preferences)
2. File creation, modification, and deletion across user directories (500+ files for Spotlight indexing)
3. Application downloads (DMG-based installs) and first-launch events for Quarantine recording
4. System service interaction (Launch Services registration, FSEvents logging)
5. Controlled shutdown and reboot (at least 2 boot cycles for unified log boot-ID rotation)

The VM is snapshotted post-exercise; artifact files are extracted, sanitized of PII, and committed with provenance documentation.

## Fixture Requirements

Each fixture entry below describes the artifact type, expected source file(s), a SHA-256 placeholder (to be filled at commit time), and the expected coverage/assertion set.

---

### 1. plist

**Source files:**
- `~/Library/Preferences/com.apple.finder.plist` (binary plist)
- `~/Library/Preferences/com.apple.Terminal.plist` (binary plist)
- `~/Library/Preferences/com.apple.Safari.plist` (binary plist)
- `~/Library/Preferences/com.apple.dock.plist` (binary plist)
- `~/Library/Preferences/com.apple.loginwindow.plist` (binary plist)
- `~/Library/Preferences/com.apple.systempreferences.plist` (binary plist)
- `/Library/Preferences/com.apple.SoftwareUpdate.plist` (binary plist)
- `/Library/Preferences/SystemConfiguration/com.apple.smb.server.plist` (binary plist)
- `~/Library/Preferences/com.apple.Bluetooth.plist` (binary plist)
- `~/Library/LaunchAgents/com.example.test.plist` (XML plist, launch agent)
- `~/Library/Preferences/.GlobalPreferences.plist` (binary plist)
- Generated XML plist test file with nested arrays, dicts, dates, and data blobs

**Description:**
A curated set of 12+ plist files (binary bplist format and XML format) extracted from the controlled macOS VM. Covers the most forensically relevant preference domains: Finder (recent items, sidebar), Terminal (window groups, shell), Safari (state), Dock (persistent apps), loginwindow (last user), System Preferences (panel history), Software Update, SMB server config, Bluetooth (paired/connected devices), global preferences, and a synthetic Launch Agent plist for launchd registration testing. At least 2 files are in XML format to exercise both parsing paths.

**SHA-256 (placeholder):**
```
<insert SHA-256 of plist collection tarball at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Total plist files | >= 12 |
| Binary (bplist) | >= 10 files |
| XML plist | >= 2 files (one launch agent, one preferences) |
| plist types exercised | `CFString`, `CFNumber`, `CFBoolean`, `CFDate`, `CFData`, `CFArray`, `CFDictionary`, `CFNull` |
| Nested depth | At least 2 plists with dictionary nesting depth >= 3 |
| Date values | >= 5 date fields across files (verify ISO-8601 or epoch conversion) |
| Boolean handling | `true` and `false` values in at least 3 files |
| Integer vs float | Both integer and floating-point `CFNumber` values present |
| Large data blobs | At least 1 `CFData` value exceeding 1 KB (e.g., bookmark data) |
| Binary plist header | `bplist00` magic; version byte parsed correctly (00 vs 10 vs 15) |
| Key forensic fields | `FXRecentFolders`, `NSNavLastRootDirectory`, `NSNavRecentPlaces`, `RecentSearches`, `DSKDesktopPref` |

**Alignment baseline:**
```json
{
  "fixture": "plist collection",
  "expected": {
    "min_files": 12,
    "min_binary": 10,
    "min_xml": 2,
    "required_types": ["CFString", "CFNumber", "CFBoolean", "CFDate", "CFData", "CFArray", "CFDictionary"],
    "max_nesting_depth_at_least": 3,
    "has_large_data_blobs": true,
    "covers_forensic_keys": ["FXRecentFolders", "NSNavLastRootDirectory"]
  }
}
```

---

### 2. unified log

**Source files:**
- `/private/var/db/diagnostics/tracev3` (directory or archive)
- `Persist/`, `Special/`, `HighVolume/` subdirectories within tracev3

**Description:**
Unified log (tracev3 format) export from a macOS VM capturing 1000+ log entries across the full lifecycle: boot, user login, GUI session activity (Safari browsing, Terminal commands, Finder file operations), application launches, network events, and shutdown/reboot. Includes at least 2 boot cycles for tracev3 boot-UUID rotation testing. Entries span multiple subsystems (com.apple.safari, com.apple.finder, com.apple.networking, com.apple.securityd, etc.).

**SHA-256 (placeholder):**
```
<insert SHA-256 of unified log collection at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Total log entries | >= 1000 |
| Distinct subsystems | >= 5 |
| Fields per entry | `timestamp`, `machTimestamp`, `process`, `pid`, `tid`, `subsystem`, `category`, `messageType`, `sender`, `message` |
| Boot UUIDs | >= 2 distinct boot UUIDs |
| Message types | `Default`, `Info`, `Debug`, `Error`, `Fault` (>= 3 distinct types) |
| Thread IDs | >= 20 distinct `tid` values |
| Process lifecycle | At least 5 processes with both first-appearance and exit entries |
| Activity IDs | `activityID` / `parentActivityID` present for chained log entries |
| Oversize messages | At least 1 oversize message (>256 bytes) if stored in separate file |
| Timestamp ordering | Entries monotonic within each log segment; cross-segment boundaries handled |
| Chunk format | Chunk header (`chunk-*`, `logdata.LiveData.*.tracev3`) parsed correctly |

**Alignment baseline:**
```json
{
  "fixture": "tracev3 (aggregate)",
  "expected": {
    "min_entries": 1000,
    "min_subsystems": 5,
    "min_boot_uuids": 2,
    "min_message_types": 3,
    "min_distinct_pids": 20,
    "has_activity_chains": true,
    "has_oversize_messages": true
  }
}
```

---

### 3. Spotlight

**Source files:**
- `/.Spotlight-V100/` (volume-level metadata)
- `~/.Spotlight-V100/` (user-level metadata)
- `/.Spotlight-V100/store.db` (Spotlight index database)
- `~/.Spotlight-V100/store.db` (user Spotlight index)

**Description:**
Spotlight index files from a macOS VM with a user home directory containing 500+ files (documents, images, downloads, applications). The index captures metadata for each indexed file: file path, display name, content type, dates (created, modified, last opened), authors, and keywords. Exercise the .store.db SQLite-based index format used in modern macOS.

**SHA-256 (placeholder):**
```
<insert SHA-256 of Spotlight collection at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Indexed file entries | >= 500 |
| Distinct content types | >= 30 (`public.plain-text`, `public.jpeg`, `public.html`, `com.apple.mail.emlx`, etc.) |
| Metadata attributes | `kMDItemPath`, `kMDItemDisplayName`, `kMDItemKind`, `kMDItemContentType`, `kMDItemFSContentChangeDate`, `kMDItemFSCreationDate`, `kMDItemFSName`, `kMDItemFSSize`, `kMDItemLastUsedDate`, `kMDItemAuthors`, `kMDItemWhereFroms` |
| store.db schema | Tables: `metadata`, `subdb_keys`, `kMDItemSubDBKeys`; parser accesses indexed content correctly |
| Volume-level vs user-level | Both `/.Spotlight-V100/` and `~/.Spotlight-V100/` present with distinct content |
| Deleted file metadata | At least 5 entries for files that have been deleted (path may no longer resolve on host filesystem) |
| Date fields | All date-type attributes parse to correct epoch or ISO-8601 values |
| Large text content | At least 3 files with full-text content indexed (kMDItemTextContent available) |

**Alignment baseline:**
```json
{
  "fixture": "Spotlight (aggregate)",
  "expected": {
    "min_indexed_files": 500,
    "min_content_types": 30,
    "required_attributes": [
      "kMDItemPath", "kMDItemDisplayName", "kMDItemKind",
      "kMDItemContentType", "kMDItemFSContentChangeDate",
      "kMDItemFSCreationDate", "kMDItemFSSize", "kMDItemLastUsedDate"
    ],
    "has_volume_spotlight": true,
    "has_user_spotlight": true,
    "has_deleted_entries": true,
    "has_fulltext_index": true
  }
}
```

---

### 4. Quarantine

**Source files:**
- `~/Library/Preferences/com.apple.LaunchServices.QuarantineEventsV2`
- (SQLite database; may also appear as plain file)

**Description:**
QuarantineEventsV2 SQLite database from a macOS VM with 50+ download/execution events. Captures file downloads from Safari, Chrome, and command-line tools (`curl`). Includes DMG mounting from browser downloads, application first-launch quarantine checks, and Gatekeeper evaluation events. Records should span multiple days and include diverse source applications.

**SHA-256 (placeholder):**
```
<insert SHA-256 of QuarantineEventsV2 at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Total quarantine events | >= 50 |
| Download sources | URL-type values in `LSQuarantineDataURLString` and `LSQuarantineOriginURLString` |
| Source applications | >= 3 distinct `LSQuarantineAgentBundleIdentifier` values (e.g., `com.apple.Safari`, `com.google.Chrome`, `com.apple.curl`) |
| Event types | `LSQuarantineType` values: `WebDownload`, `OtherDownload`, `AppDownload` |
| Timestamps | `LSQuarantineTimeStamp` (CF Absolute Time or epoch), all events ordered |
| File paths | `LSQuarantineDataURLString` resolves to actual file paths (at time of event) |
| Gatekeeper | At least 5 events where `LSQuarantineEventIdentifier` is populated (Gatekeeper evaluation) |
| Deleted files | At least 5 entries where the referenced file no longer exists on disk |
| Table schema | `LSQuarantineEvent` table with expected columns: `LSQuarantineEventIdentifier`, `LSQuarantineTimeStamp`, `LSQuarantineAgentBundleIdentifier`, `LSQuarantineAgentName`, `LSQuarantineDataURLString`, `LSQuarantineSenderName`, `LSQuarantineOriginURLString`, `LSQuarantineTypeNumber` |

**Alignment baseline:**
```json
{
  "fixture": "QuarantineEventsV2",
  "expected": {
    "min_events": 50,
    "min_source_apps": 3,
    "has_url_sources": true,
    "has_gatekeeper_events": true,
    "has_deleted_file_references": true,
    "required_columns": [
      "LSQuarantineEventIdentifier", "LSQuarantineTimeStamp",
      "LSQuarantineAgentBundleIdentifier", "LSQuarantineDataURLString",
      "LSQuarantineOriginURLString", "LSQuarantineTypeNumber"
    ]
  }
}
```

---

### 5. Launch Services

**Source files:**
- `/private/var/db/launchd.db/com.apple.launchd/overrides.plist`
- `~/Library/Preferences/com.apple.LaunchServices.plist` (LSQuarantine preferences not required here — focus on app registration)
- `~/Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist`
- `/System/Library/CoreServices/CoreTypes.bundle/Contents/Info.plist` (UTI declarations, optional reference)

**Description:**
Launch Services application registration database from a macOS VM with multiple third-party applications installed (browser, text editor, media player). Captures the bundle identifier → path mapping, file type associations, and UTI declarations that Launch Services maintains.

**SHA-256 (placeholder):**
```
<insert SHA-256 of Launch Services collection at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Registered applications | >= 15 application bundle IDs |
| Launch Services plist entries | >= 30 key-value pairs (app handlers, UTI handlers) |
| Bundle identifier diversity | At least 5 `com.apple.*` and 5 third-party identifiers |
| UTI associations | At least 10 distinct UTI ↔ application handler mappings |
| Handler role types | `Viewer`, `Editor`, `All` roles represented |
| Secure LS database | `com.apple.launchservices.secure.plist` present and parsed (macOS 10.15+/Secure Kernel Extension Loading era) |
| launchd overrides | `/private/var/db/launchd.db/` plist present for launch daemon config |
| Date fields | Application registration/last-used dates present |
| URL scheme handlers | At least 3 URL scheme ↔ application mappings |

**Alignment baseline:**
```json
{
  "fixture": "Launch Services (aggregate)",
  "expected": {
    "min_registered_apps": 15,
    "min_bundle_ids": 10,
    "min_uti_associations": 10,
    "handler_roles": ["Viewer", "Editor", "All"],
    "has_secure_ls": true,
    "has_launchd_overrides": true,
    "has_url_scheme_handlers": true
  }
}
```

---

### 6. FSEvents

**Source files:**
- `/.fseventsd/` directory content:
  - `fseventsd-uuid` (volume UUID)
  - `0000000000000001` through `000000000000000N` (event log pages)
  - `sl_evt_*` files (if present on journaled APFS volume)

**Description:**
FSEvents log directory from a macOS VM with 1000+ file system change events recorded across a scripted activity session. Captures file creation, modification, deletion, rename, and metadata changes. Events span both system and user directories. Includes at least 2 event log pages to exercise page-boundary parsing.

**SHA-256 (placeholder):**
```
<insert SHA-256 of fseventsd collection at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Total FSEvent records | >= 1000 |
| Event types | `FSE_CREATE_FILE`, `FSE_REMOVE`, `FSE_RENAME`, `FSE_CONTENT_MODIFIED`, `FSE_EXCHANGE`, `FSE_CHOWN` (>= 4 distinct types) |
| Distinct paths | >= 200 distinct file/directory paths |
| Event log pages | >= 2 pages (files `0000000000000001`, `0000000000000002`, etc.) |
| Volume UUID | `fseventsd-uuid` file present with valid UUID |
| Timestamp accuracy | Events within expected activity window; timestamps non-decreasing within each page |
| Path depth | At least 10 paths with nesting depth >= 4 |
| System paths | Events in `/System/`, `/Library/`, or `/private/` directories |
| User paths | Events in `~/Documents/`, `~/Downloads/`, or `~/Desktop/` |
| Rename tracking | At least 10 rename event pairs (old path → new path correlate) |
| Deleted file events | At least 30 `FSE_REMOVE` events |

**Alignment baseline:**
```json
{
  "fixture": "fseventsd (aggregate)",
  "expected": {
    "min_events": 1000,
    "min_event_types": 4,
    "min_distinct_paths": 200,
    "min_log_pages": 2,
    "has_volume_uuid": true,
    "covers_system_and_user_paths": true,
    "has_rename_events": true,
    "has_delete_events": 30
  }
}
```

---

## Size Constraints

Per the public-medium tier policy (`testdata/fixtures/public-medium/README.md`), individual files should be under 10 MB. The macOS artifact collection here should total under 30 MB across all files. Spotlight store.db indexes may approach 5-8 MB; unified log tracev3 data may also approach that range. Individual plist files are typically under 100 KB each.

## Relationship to Other Fixture Tiers

| Tier | Directory | Relevant macOS Artifacts |
|------|-----------|--------------------------|
| Public Small | `testdata/fixtures/public-small/` | Minimal synthetic plists and log samples for unit-level parser smoke tests |
| **Public Medium** | `testdata/fixtures/public-medium/macos/` | **This directory.** VM-sourced artifacts for integration-level regression |
| Private Real | `testdata/fixtures/private-real-regression/` | Full APFS images, production Mac workstation logs (not committed) |

## Related Documentation

- `testdata/fixtures/public-medium/README.md` — tier policy and naming conventions
- `docs/parser-support-matrix.md` — per-parser support level and fixture status
- `docs/mac-artifact-coverage.md` — macOS parser design and field-level commitments
- `docs/v3-plan.md` — V3 roadmap for macOS artifact support
