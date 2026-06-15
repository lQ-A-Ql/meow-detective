# Public-Medium Browser Artifact Fixtures

`testdata/fixtures/public-medium/browser/` contains medium-sized browser forensic artifacts sourced from controlled browsing session VM snapshots. These fixtures are checked into the repository for CI-compatible regression testing of browser history, download, and cookie parsers.

## Purpose

- Validate browser artifact parser correctness at medium scale (hundreds to thousands of records)
- Exercise multi-profile scenarios, download metadata, cookie expiration, and URL diversity
- Provide alignment baselines for automated assertion tests
- Enable cross-browser correlation (Chrome ↔ Edge ↔ Firefox timeline alignment)

## Acquisition Source

All fixtures are to be sourced from a controlled Windows VM (Windows 10/11) with Chrome, Edge, and Firefox installed, or from separate platform-appropriate VMs. The VM undergoes a scripted browsing session:

1. **Browsing phase**: 300+ distinct URLs visited across news, social media, search engines, developer documentation, video platforms, and file-sharing sites. Sessions span multiple days with timestamps reflecting realistic browsing patterns (gaps, bursts, cross-session continuation).
2. **Download phase**: 30+ file downloads of varying types (PDF, ZIP, EXE, MSI, image, video) initiated from browser. Some are completed, some cancelled or interrupted.
3. **Cookie phase**: 100+ cookies set across visited domains, including first-party, third-party (tracking), session cookies, and persistent cookies with varied expiration dates.
4. **Profile phase**: At least one browser configured with 2+ profiles (e.g., personal + work) to test profile isolation.
5. **Extensions**: At least 3 extensions installed with web-accessible content where applicable.

The VM is snapshotted post-exercise; browser profile directories are extracted, sanitized of PII (replace usernames, machine names, local file paths in download records), and committed with provenance documentation.

---

## Fixture Requirements

### 1. Chrome

**Source directory:** `%LOCALAPPDATA%\Google\Chrome\User Data\`

**Key files:**
- `Default/History` — SQLite database: `urls`, `visits`, `visit_source`, `downloads`, `downloads_url_chains` tables
- `Default/Cookies` — SQLite database: `cookies` table
- `Default/Login Data` — SQLite database (excluded from fixture; PII-sensitive)
- `Default/Web Data` — SQLite database: autofill, keywords (search terms excluded from fixture)
- `Default/Preferences` — JSON: profile metadata, extension list
- `Default/Extensions/` — extension directories (metadata only; CRX payloads excluded)
- `Profile 1/History`, `Profile 1/Cookies` — second profile data (if multi-profile configured)

**SHA-256 (placeholder):**
```
<insert SHA-256 of Chrome profile tarball at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| History entries (urls table) | >= 500 rows |
| Visit records (visits table) | >= 800 (each URL may have multiple visits) |
| Distinct domains | >= 80 |
| Visit transition types | `link`, `typed`, `auto_bookmark`, `auto_subframe`, `generated`, `reload`, `keyword`, `keyword_generated` (>= 5 distinct) |
| Download records (downloads table) | >= 50 |
| Download states | >= 3 distinct states (`1`=complete, `2`=interrupted, `3`=cancelled, `4`=interrupted) |
| Downloaded file types | >= 10 distinct file extensions |
| Download URL chains | `downloads_url_chains` table present with multi-redirect chains (>= 3 entries with chain_index > 0) |
| Cookie records | >= 100 |
| Cookie attributes | `host_key`, `name`, `value`, `path`, `expires_utc`, `is_secure`, `is_httponly`, `last_access_utc`, `has_expires`, `is_persistent`, `priority`, `encrypted_value`, `samesite`, `source_scheme` |
| Expired vs active cookies | Mixed: at least 10 expired, at least 50 not expired |
| Secure cookies | >= 15 with `is_secure=1` |
| HttpOnly cookies | >= 10 with `is_httponly=1` |
| Third-party cookies | >= 20 cookies where domain differs from top-level site |
| Chrome version | `last_version` in Preferences >= 100 |
| Extensions | >= 3 entries in `extensions.settings` within Preferences |
| Profile count | >= 2 (Default + Profile 1) with distinct History files |
| Timestamp format | Chrome WebKit/Windows epoch (microseconds since 1601-01-01) parsed correctly |
| SQLite WAL handling | Parser handles `History-wal` and `History-journal` if present |

**Alignment baseline:**
```json
{
  "fixture": "Chrome (aggregate)",
  "expected": {
    "min_history_entries": 500,
    "min_visits": 800,
    "min_distinct_domains": 80,
    "min_downloads": 50,
    "min_cookies": 100,
    "min_profiles": 2,
    "min_extensions": 3,
    "chrome_version": ">= 100",
    "required_tables": ["urls", "visits", "visit_source", "downloads", "downloads_url_chains", "meta"],
    "required_cookie_attributes": [
      "host_key", "name", "expires_utc", "is_secure", "is_httponly",
      "last_access_utc", "is_persistent", "samesite"
    ]
  }
}
```

---

### 2. Edge (Chromium)

**Source directory:** `%LOCALAPPDATA%\Microsoft\Edge\User Data\`

**Key files:**
- `Default/History` — SQLite database (same schema as Chrome)
- `Default/Cookies` — SQLite database (same schema as Chrome)
- `Default/Preferences` — JSON: profile metadata
- `Default/Collections/` — Collections database (Edge-specific feature, if populated)

**SHA-256 (placeholder):**
```
<insert SHA-256 of Edge profile tarball at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| History entries | >= 300 |
| Visit records | >= 500 |
| Distinct domains | >= 50 |
| Download records | >= 20 |
| Cookie records | >= 80 |
| Edge-specific detection | Edge profile path correctly identified (not misidentified as Chrome) |
| Profile metadata | Preferences JSON includes `edge`-specific keys (`edge.*` profile settings) |
| Collections (optional) | If Collections feature used, `collections` table in Collections SQLite parsed |
| Timestamp format | Same Chrome WebKit epoch; correctly parsed |
| Schema compatibility | Parser handles Edge's Chrome-compatible SQLite schema without Chrome-specific assumptions that break |

**Alignment baseline:**
```json
{
  "fixture": "Edge (aggregate)",
  "expected": {
    "min_history_entries": 300,
    "min_visits": 500,
    "min_downloads": 20,
    "min_cookies": 80,
    "browser_type": "edge_chromium",
    "requires_edge_detection": true,
    "schema_compatible_with_chrome": true
  }
}
```

---

### 3. Firefox

**Source directory:** `%APPDATA%\Mozilla\Firefox\Profiles\<profile-id>.default-release\`

**Key files:**
- `places.sqlite` — SQLite database: `moz_places`, `moz_historyvisits`, `moz_bookmarks`, `moz_inputhistory`, `moz_origins` tables
- `downloads.json` — JSON array of download records (Firefox >= 98) or `places.sqlite` `moz_annos` entries
- `cookies.sqlite` — SQLite database: `moz_cookies` table
- `extensions.json` — JSON: extension metadata and permissions
- `prefs.js` — user preferences (JavaScript-like format)
- `logins.json` — encrypted login records (excluded from fixture; PII-sensitive)

**SHA-256 (placeholder):**
```
<insert SHA-256 of Firefox profile tarball at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| History entries (moz_places) | >= 300 URLs |
| Visit records (moz_historyvisits) | >= 500 visits |
| Distinct domains | >= 50 unique host values |
| Visit types | `TRANSITION_LINK(1)`, `TRANSITION_TYPED(2)`, `TRANSITION_BOOKMARK(3)`, `TRANSITION_EMBED(4)`, `TRANSITION_REDIRECT_PERMANENT(5)`, `TRANSITION_REDIRECT_TEMPORARY(6)`, `TRANSITION_DOWNLOAD(7)`, `TRANSITION_FRAMED_LINK(8)`, `TRANSITION_RELOAD(9)` (>= 5 distinct) |
| Download records (downloads.json) | >= 30 |
| Download fields | `url`, `target.path`, `target.size`, `startTime`, `endTime`, `totalBytes`, `state`, `referrerInfo`, `fileSize`, `type`, `source` |
| Download states | `0`=Downloading, `1`=Finished, `2`=Failed, `3`=Canceled, `4`=Paused, `5`=BlockedParental, `6`=BlockedPolicy (>= 3 distinct) |
| Cookie records | >= 80 |
| Cookie attributes | `baseDomain`, `originAttributes`, `name`, `value`, `host`, `path`, `expiry`, `lastAccessed`, `creationTime`, `isSecure`, `isHttpOnly`, `inBrowserElement`, `sameSite`, `rawSameSite`, `schemeMap` |
| Firefox bookmarks | >= 10 bookmark records in `moz_bookmarks` |
| Favicons | `moz_favicons` table populated |
| Extensions | >= 3 extensions in `extensions.json` |
| Firefox version | `prefs.js` or `compatibility.ini` includes version >= 98 |
| Timestamp format | Firefox PRTime (microseconds since 1970-01-01) parsed correctly |
| SQLite WAL handling | `places.sqlite-wal` handled if present |
| Downloads JSON format | `downloads.json` is valid JSON array; parser tolerates empty entries and trailing whitespace |
| `target.fileSize` | At least 5 downloads with fileSize != totalBytes (partial/cancelled downloads) |

**Alignment baseline:**
```json
{
  "fixture": "Firefox (aggregate)",
  "expected": {
    "min_history_entries": 300,
    "min_visits": 500,
    "min_downloads": 30,
    "min_cookies": 80,
    "min_bookmarks": 10,
    "min_extensions": 3,
    "required_tables": ["moz_places", "moz_historyvisits", "moz_bookmarks", "moz_cookies"],
    "downloads_format": "json_array",
    "required_download_fields": [
      "url", "target.path", "startTime", "endTime", "totalBytes", "state"
    ],
    "firefox_version": ">= 98"
  }
}
```

---

## Cross-Browser Correlation Requirements

When all three browser fixtures are loaded into a case, the following integration assertions should hold:

1. **Timeline ordering**: Events from all three browsers can be merged into a single chronologically ordered timeline.
2. **URL normalization**: Same domain visited in multiple browsers produces comparable entries (hostname, path, query).
3. **Time zone handling**: Timestamps are normalized to a common epoch (UTC) regardless of browser-specific epoch base (1601 vs 1970).
4. **Download dedup awareness**: Downloads of identical files from different browsers are distinguishable.
5. **Search extraction**: Search query parameter extraction works for all three browsers (Google, Bing, DuckDuckGo, etc.).

---

## Size Constraints

Per the public-medium tier policy (`testdata/fixtures/public-medium/README.md`), individual files should be under 10 MB. Browser SQLite databases can grow with browsing activity; target profile sizes:

| Browser | Typical profile size | Notes |
|---------|---------------------|-------|
| Chrome | 2-8 MB | History.sqlite ~1-3 MB with 500+ entries; Cookies.sqlite <1 MB |
| Edge | 1-4 MB | Typically smaller than equivalent Chrome profile |
| Firefox | 1-5 MB | places.sqlite ~1-3 MB; downloads.json ~10-50 KB |

If total per-browser size exceeds 5 MB, use SQLite VACUUM on History/Cookies/places databases before committing to reduce fragmentation.

## PII Sanitization Checklist

Before committing browser fixtures, ensure:

- [ ] Usernames in download paths replaced with placeholder (e.g., `C:\Users\ForensicUser\...`)
- [ ] Machine name in paths replaced with placeholder (e.g., `DESKTOP-TEST`)
- [ ] Login credentials excluded (`Login Data` SQLite, `logins.json` excluded)
- [ ] Autofill data replaced with placeholders or excluded
- [ ] Search keyword history replaced with generic terms
- [ ] Email addresses in cookies replaced with placeholders
- [ ] Real personal API keys, tokens, or session identifiers scrubbed
- [ ] Browser sync account metadata removed
- [ ] Extension-specific identifiable data reviewed and sanitized

## Relationship to Other Fixture Tiers

| Tier | Directory | Relevant Browser Artifacts |
|------|-----------|----------------------------|
| Public Small | `testdata/fixtures/public-small/` | Minimal synthetic SQLite databases with 10-20 entries for unit-level parser smoke tests |
| **Public Medium** | `testdata/fixtures/public-medium/browser/` | **This directory.** VM-sourced browser profiles for integration-level regression |
| Private Real | `testdata/fixtures/private-real-regression/` | Full user profiles from production workstations (not committed) |

## Related Documentation

- `testdata/fixtures/public-medium/README.md` — tier policy and naming conventions
- `docs/parser-support-matrix.md` — per-browser support level and fixture status
- `docs/browser-artifact-coverage.md` — browser parser design and field-level commitments
- `docs/v2-longterm-plan.md` — V2 roadmap for browser history support
