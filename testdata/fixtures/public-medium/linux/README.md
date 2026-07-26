# Public-Medium Linux Artifact Fixtures

`testdata/fixtures/public-medium/linux/` contains medium-sized Linux forensic artifacts sourced from controlled VM snapshots. These fixtures are checked into the repository for CI-compatible regression testing of Linux artifact parsers.

## Purpose

- Validate Linux parser correctness at medium scale (hundreds to thousands of records)
- Exercise edge cases: multi-user sessions, log rotation, timestamp variations, compression
- Provide alignment baselines for automated assertion tests
- Enable cross-module integration testing (timeline, search, evidence reader)

## Acquisition Source

All fixtures are to be sourced from a controlled Linux VM (Ubuntu 22.04 LTS or equivalent) that undergoes a scripted activity session:

1. Multi-user login/logout cycle (3+ users over 48-hour simulated uptime)
2. sudo usage across users (install, configure, service management)
3. apt/dpkg operations (full system upgrade, package install/remove/purge)
4. Extensive bash usage with HISTTIMEFORMAT enabled
5. Cron job scheduling (system + per-user crontabs)
6. systemd journal capturing full boot→login→activity→shutdown lifecycle

The VM is snapshotted post-exercise; artifact files are extracted, sanitized of PII, and committed with provenance documentation.

## Fixture Requirements

Each fixture entry below describes the artifact type, expected source file(s), a SHA-256 placeholder (to be filled at commit time), and the expected coverage/assertion set.

---

### 1. systemd journal

**Source files:**
- `/var/log/journal/<machine-id>/system.journal`
- `/var/log/journal/<machine-id>/user-1000.journal`

**Description:**
Journal export from a multi-boot VM session with 1000+ log entries. Captures kernel messages, systemd unit lifecycle, service starts/stops, user sessions, and application logs across at least two boot cycles (to exercise boot ID rotation). Entry types include `_TRANSPORT=journal`, `_TRANSPORT=stdout`, and `_TRANSPORT=syslog`.

**SHA-256 (placeholder):**
```
<insert SHA-256 of system.journal at commit time>
<insert SHA-256 of user-1000.journal at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Total entries | >= 1000 |
| Fields per entry | `__REALTIME_TIMESTAMP`, `MESSAGE`, `_PID`, `_UID`, `_GID`, `_EXE`, `_CMDLINE`, `_SYSTEMD_UNIT`, `_TRANSPORT`, `PRIORITY`, `SYSLOG_FACILITY` |
| Boot IDs | >= 2 distinct boot IDs |
| Priority levels | >= 3 distinct levels (emerg/err/warning/info/debug) |
| Transport types | >= 2 (`journal`, `stdout`) |
| Compressed fields | Verify decompression of LZ4-compressed field objects (journal >= v189) |
| Monotonic timestamps | `__MONOTONIC_TIMESTAMP` present, monotonically non-decreasing within each boot |
| Cursor | `__CURSOR` field present and parseable |

**Alignment baseline (excerpt):**
```json
{
  "fixture": "system.journal",
  "expected": {
    "min_entries": 1000,
    "min_boot_ids": 2,
    "required_fields": [
      "__REALTIME_TIMESTAMP", "MESSAGE", "_PID", "_UID", "_GID",
      "_EXE", "_CMDLINE", "_SYSTEMD_UNIT", "_TRANSPORT", "PRIORITY"
    ],
    "boot_entry_count_ratio": "each boot >= 200 entries"
  }
}
```

---

### 2. wtmp

**Source files:**
- `/var/log/wtmp`
- `/var/log/btmp` (failed login attempts)

**Description:**
wtmp binary log from a multi-user VM session with 100+ login/logout records. Includes successful logins via console, SSH, and `su`, along with system boot/shutdown markers. The companion `btmp` file captures failed authentication attempts.

**SHA-256 (placeholder):**
```
<insert SHA-256 of wtmp at commit time>
<insert SHA-256 of btmp at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Total records (wtmp) | >= 100 |
| Failed records (btmp) | >= 10 |
| Record types | `UT_UNKNOWN(0)`, `RUN_LVL(1)`, `BOOT_TIME(2)`, `NEW_TIME(3)`, `OLD_TIME(4)`, `INIT_PROCESS(5)`, `LOGIN_PROCESS(6)`, `USER_PROCESS(7)`, `DEAD_PROCESS(8)`, `ACCOUNTING(9)` |
| User count | >= 3 distinct users |
| Terminal types | >= 3 (`tty1`, `pts/0`, `:0`) |
| Host strings | At least one remote host IP present |
| Timestamp ordering | Records monotonically non-decreasing by `tv_sec` |
| Session pairing | Each `USER_PROCESS` has corresponding `DEAD_PROCESS` (except still-logged-in sessions at snapshot time) |

**Alignment baseline:**
```json
{
  "fixture": "wtmp",
  "expected": {
    "min_records": 100,
    "min_users": 3,
    "required_record_types": [0, 1, 2, 5, 6, 7, 8],
    "has_remote_sessions": true,
    "has_boot_events": true
  }
}
```

---

### 3. bash history

**Source files:**
- `/home/<user1>/.bash_history`
- `/home/<user2>/.bash_history`
- `/root/.bash_history`

**Description:**
bash history files with 500+ total commands across users, captured with `HISTTIMEFORMAT="%F %T "` enabled. Includes common forensic patterns: package management, file operations, network diagnostics, service control, user switching, and editor invocations.

**SHA-256 (placeholder):**
```
<insert SHA-256 of user1 .bash_history at commit time>
<insert SHA-256 of user2 .bash_history at commit time>
<insert SHA-256 of root .bash_history at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Total commands | >= 500 across all history files |
| Timestamped entries | >= 80% of entries have `#<epoch>` preamble |
| User count | >= 3 distinct users (including root) |
| Command diversity | >= 50 distinct base commands (first token) |
| Long commands | At least one command exceeding 200 characters |
| Multiline commands | At least 2 commands containing escaped newlines or heredocs |
| Sudo commands | >= 20 commands prefixed with `sudo` |
| Empty lines | Parser handles empty lines and whitespace-only lines |
| Encoding | UTF-8; at least 5 entries with non-ASCII characters (e.g., file names) |

**Alignment baseline:**
```json
{
  "fixture": ".bash_history (aggregate)",
  "expected": {
    "min_commands": 500,
    "min_users": 3,
    "has_timestamps": true,
    "timestamp_format": "epoch_seconds",
    "min_sudo_commands": 20,
    "min_distinct_commands": 50
  }
}
```

---

### 4. apt/dpkg history

**Source files:**
- `/var/log/apt/history.log`
- `/var/log/apt/term.log`
- `/var/log/dpkg.log`
- `/var/log/dpkg.log.1` (rotated log)

**Description:**
apt and dpkg logs capturing a full system upgrade cycle: initial point-release state, `apt update`, `apt full-upgrade` involving 200+ package events (installs, upgrades, removals, purges, configuration changes), and post-upgrade cleanup. Includes at least one rotated `dpkg.log.1` to exercise log rotation handling.

**SHA-256 (placeholder):**
```
<insert SHA-256 of history.log at commit time>
<insert SHA-256 of dpkg.log at commit time>
<insert SHA-256 of dpkg.log.1 at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Total package events (dpkg.log) | >= 200 |
| Operations | `install`, `upgrade`, `remove`, `purge`, `configure`, `trigproc` |
| Unique packages | >= 100 distinct package names |
| apt history transactions | >= 5 transactions in history.log (Start-Date/End-Date blocks) |
| Command line recorded | `Commandline:` field present in each apt transaction |
| Log rotation | dpkg.log.1 present with entries predating dpkg.log |
| Timestamp monotonicity | Within each log file, timestamps are non-decreasing |
| Architecture coverage | Packages with `amd64`, `all` architectures; at least one `i386` |

**Alignment baseline:**
```json
{
  "fixture": "dpkg.log",
  "expected": {
    "min_events": 200,
    "required_operations": ["install", "upgrade", "remove", "purge", "configure"],
    "min_unique_packages": 100,
    "has_rotated_log": true
  }
}
```

---

### 5. cron

**Source files:**
- `/var/spool/cron/crontabs/<user1>`
- `/var/spool/cron/crontabs/<user2>`
- `/etc/crontab`
- `/etc/cron.d/anacron`
- `/etc/cron.d/e2scrub_all`
- `/etc/cron.daily/` (at least 3 scripts)
- `/etc/cron.hourly/` (at least 1 script)
- `/etc/cron.weekly/` (at least 1 script)
- `/etc/cron.monthly/` (at least 1 script)

**Description:**
System and per-user crontab definitions from a VM configured with standard cron (not systemd timers). Includes daily, hourly, weekly, and monthly cron directories populated with distribution-default maintenance scripts, plus at least 2 user-level crontabs with 5+ job definitions each. Total of 20+ job definitions across all sources.

**SHA-256 (placeholder):**
```
<insert SHA-256 of crontab collection tarball at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Total job definitions | >= 20 (sum across all crontab sources) |
| User crontabs | >= 2 distinct user crontab files |
| System crontab | `/etc/crontab` present with at least 3 jobs |
| cron.d entries | >= 2 files in `/etc/cron.d/` with valid schedules |
| cron.hourly entries | >= 1 script |
| cron.daily entries | >= 3 scripts |
| cron.weekly entries | >= 1 script |
| cron.monthly entries | >= 1 script |
| Schedule expression diversity | >= 5 distinct crontab schedule patterns (different minutes/hours/dom/mon/dow) |
| Comment handling | At least 5 lines with `#` comments preceding job definitions |
| Environment variables | At least 2 lines setting env vars (`SHELL=`, `PATH=`, `MAILTO=`) |
| @-syntax support | At least 1 `@reboot` entry; at least 1 `@daily` or `@hourly` entry |

**Alignment baseline:**
```json
{
  "fixture": "cron (aggregate)",
  "expected": {
    "min_job_definitions": 20,
    "min_user_crontabs": 2,
    "has_system_crontab": true,
    "has_cron_d": true,
    "has_cron_hourly": true,
    "has_cron_daily": true,
    "has_cron_weekly": true,
    "has_cron_monthly": true,
    "min_distinct_schedules": 5,
    "has_reboot_entry": true,
    "has_env_vars": true
  }
}
```

---

### 6. sudo

**Source file:**
- `/var/log/auth.log`

**Description:**
auth.log from a VM with 50+ sudo session events, including successful authentications, failed password attempts, command execution records, session open/close events, and PAM session transitions. Captures sudo usage across multiple users with varying command complexity.

**SHA-256 (placeholder):**
```
<insert SHA-256 of auth.log at commit time>
```

**Expected coverage:**

| Assertion category | Minimum count / check |
|---|---|
| Total sudo events | >= 50 |
| Successful sudo sessions | >= 35 (USER=root ; COMMAND=...) |
| Failed sudo attempts | >= 5 (authentication failure / 3 incorrect password attempts) |
| Distinct sudo users | >= 2 |
| Distinct commands | >= 15 unique commands executed via sudo |
| Session open/close | >= 20 pairs of `session opened` / `session closed` for sudo |
| PAM transitions | At least 10 PAM-related lines adjacent to sudo entries |
| Timestamp monotonicity | Events within auth.log are monotonically non-decreasing |
| Long commands | At least 1 sudo command exceeding 150 characters |
| Environment passthrough | At least 1 sudo invocation with `env` or environment variable setting |
| Non-interactive sudo | At least 1 command logged from non-TTY context (e.g., cron) |

**Alignment baseline:**
```json
{
  "fixture": "auth.log",
  "expected": {
    "min_sudo_events": 50,
    "min_successful_sessions": 35,
    "min_failed_attempts": 5,
    "min_distinct_users": 2,
    "min_distinct_commands": 15,
    "has_session_pairs": true,
    "has_pam_records": true,
    "has_long_commands": true
  }
}
```

---

## Size Constraints

Per the public-medium tier policy (`testdata/fixtures/public-medium/README.md`), individual files should be under 10 MB. The Linux artifact collection here should total under 25 MB across all files. systemd journal may approach but should not exceed 8 MB; log rotation (dpkg.log.1) keeps individual log files small.

## Relationship to Other Fixture Tiers

| Tier | Directory | Relevant Linux Artifacts |
|------|-----------|--------------------------|
| Public Small | `testdata/fixtures/public-small/` | Minimal synthetic samples for unit-level parser smoke tests |
| **Public Medium** | `testdata/fixtures/public-medium/linux/` | **This directory.** VM-sourced artifacts for integration-level regression |
| Private Real | `testdata/fixtures/private-real-regression/` | Full-disk images, production server logs (not committed) |

## Related Documentation

- `testdata/fixtures/public-medium/README.md` — tier policy and naming conventions
- `docs/parser-support-matrix.md` — per-parser support level and fixture status
- `docs/linux-artifact-coverage.md` — Linux parser design and field-level commitments
- `docs/v3-plan.md` — V3 roadmap for Linux artifact support
