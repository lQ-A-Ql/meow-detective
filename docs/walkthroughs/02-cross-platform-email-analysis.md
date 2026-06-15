# Walkthrough: Cross-Platform Email Analysis

This walkthrough demonstrates email forensic analysis using Forensics Workbench V3 with PST and mbox evidence containers. The scenario involves an Outlook PST file from a Windows workstation and a Thunderbird mbox archive from a macOS laptop — both belonging to the same person of interest — loaded into a single case for cross-source correlation.

## 0. Prerequisites

- Forensics Workbench V3 installed
- Evidence files: `outlook-archive.pst` (Unicode PST), `thunderbird-inbox.mbox` (mboxrd)
- Loaded rule pack: `email-correlation`
- Verification config at `<case_root>/verification.toml` with `required_packs = ["email-correlation"]`

---

## 1. Import PST and mbox Evidence

1. Create a new case named `Email-Investigation-2026-06`.
2. Go to **Import > Container Evidence**. Select **PST** format and choose `outlook-archive.pst`.
3. The PST import scans the internal folder hierarchy. Wait for the probe phase:

```
Probe: outlook-archive.pst
  Format: Outlook Personal Storage Table (Unicode 64-bit NDB)
  File size: 2,184,302,592 bytes (2.0 GiB)
  Encryption: none
  Internal structure:
    Root folder: "Personal Folders"
    Subfolders: 48 (Inbox, Sent Items, Deleted Items, ...)
    Total items: 34,211
    Date range: 2023-03-14 to 2026-06-10
```

4. Repeat for the mbox file. Go to **Import > Container Evidence**. Select **mbox** format and choose `thunderbird-inbox.mbox`.

```
Probe: thunderbird-inbox.mbox
  Format: mbox (detected variant: mboxrd)
  File size: 412,876,800 bytes (393 MiB)
  Total messages: 8,723
  Date range: 2024-01-02 to 2026-06-13
```

5. Start import with **full artifact extraction** and **entity extraction** enabled.

---

## 2. Browse Email Folder Structure

After import, the email containers appear as virtual file trees in the File Browser.

1. Open the **File Browser**. Both PST and mbox appear as top-level data sources.
2. Expand the PST tree:

```
outlook-archive.pst
├── Inbox (18,432 messages)
├── Sent Items (9,821 messages)
├── Deleted Items (2,108 messages)
├── Drafts (47 messages)
├── Project Alpha (1,203 messages)
├── Finance (892 messages)
└── Archive 2023-2025 (1,708 messages)
```

3. Expand the mbox tree. The mbox is presented as a folder hierarchy even though the raw format is flat:

```
thunderbird-inbox.mbox
├── Inbox (7,415 messages)
├── Sent (1,102 messages)
├── Trash (156 messages)
└── Drafts (50 messages)
```

4. Click any folder to list messages in the file listing pane. Each message is rendered as a row with sender, recipients, subject, date, and attachment count.
5. Use the **search bar** to filter messages by subject, sender, or date range across both containers simultaneously.

---

## 3. Extract Email Artifacts

The email artifact extraction produces typed records for messages, attachments, calendar entries, and contacts.

1. Go to **Artifacts** view and filter by `Artifact Family = Email`.
2. Examine the artifact breakdown:

```
Email artifacts: 42,934
  EmailMessage    : 34,211 (PST) + 8,723 (mbox)
  EmailAttachment :  5,412 (3,187 PST + 2,225 mbox)
  CalendarEvent   :    318 (PST only — mbox has no calendar)
  Contact         :    751 (PST only — mbox has no contacts)
```

3. Click an **EmailMessage** artifact to see:
   - Full headers (From, To, CC, BCC, Date, Subject, Message-ID, In-Reply-To)
   - Threading information (parent Message-ID links for conversation reconstruction)
   - Container path (e.g., `outlook-archive.pst / Project Alpha / Subproject Foo`)
   - Attachment list with names, sizes, and content-type hints
   - `message_class` (IPM.Note for standard email, IPM.Schedule.Meeting for calendar invites)

4. For **EmailAttachment** artifacts, the V3 evidence graph creates `References` edges linking the attachment artifact to its parent EmailMessage artifact, and `DerivesFrom` edges if the attachment filename matches a standalone file in the file tree (e.g., an attachment that was saved to disk and later found by NTFS catalog).

---

## 4. Correlate Email Attachments with File System Entries

The `email-correlation` rule pack matches email attachment names against the file tree by filename and file size.

1. Go to **Correlation Workspace**. Run the `email-correlation` pack.
2. Review the generated leads:

```
Correlation complete: 1,287 leads
  Strong  :   412
  Moderate:   615
  Weak    :   260

Top rule contributions:
  EmailAttachment-to-FileName :   823 leads
  EmailContact-to-Account     :   209 leads
  EmailCalendar-to-Timeline   :   142 leads
  EmailThread-Analysis        :   113 leads
```

3. **Strong lead example**: Attachment `Q2_Financials.xlsx` (524,288 bytes, SHA-256 `a1b2...`) in email `msg-48291` from `cf@example.com` sent 2026-04-15 → `CorrelatesWith` edge to `File: C:\Users\cf\Documents\Q2_Financials.xlsx` (524,288 bytes, same hash). Confidence: **Strong** (name + size + hash match; the file is the same artifact).

4. **Moderate lead example**: Attachment `invoice_2026-03.pdf` (name-only match) found in both PST sent items and the file tree. The hash differs (the sent version is a draft; the disk version is the final signed copy). Confidence: **Moderate** (name match, but hash mismatch indicates different versions).

5. **Email thread reconstruction**: The graph query below reconstructs a conversation thread from the `In-Reply-To` / `Message-ID` chain:

```json
{
  "startNode": { "nodeId": "artifact:email-48291" },
  "edgeFilters": ["Precedes"],
  "maxDepth": 0,
  "traversal": "ancestors"
}
```

This returns the full thread: initial message → reply 1 → reply 2 → reply 3 (current message).

---

## 5. Notebook Documentation of Communication Patterns

1. Open the **Notebook Panel**. Create a new **Observation** entry titled `External Communication Patterns`.
2. Document findings with cited evidence:

```markdown
# Observation: External Communication Patterns — Person of Interest (cf@example.com)

## Email Volume by External Domain (Jan–Jun 2026)
- competitor-corp.com: 1,203 messages (high volume, accelerating in Q2 2026)
- shell-company.net:   412 messages (all from "recruiter@shell-company.net")
- protonmail.com:       87 messages (personal encrypted accounts)

## Key Findings
1. **Data exfiltration vector**: 23 emails from cf@example.com to
   external addresses contained attachments matching internal filenames.
   - [Correlation Lead: EmailAttachment-to-FileName (23 Strong leads)]
   - Example: [EmailAttachment: Q2_Financials.xlsx — sent to ext@protonmail.com 2026-04-15]
   - Match: [File: C:\Users\cf\Documents\Q2_Financials.xlsx — same hash]

2. **Calendar coordination**: 12 meetings with external participants from
   competitor-corp.com (2026-04 through 2026-06).
   - [CalendarEvent: "Strategy Discussion" — 2026-05-20, 2 external attendees]
   - [CalendarEvent: "Contract Review" — 2026-06-01, 3 external attendees]

3. **Cross-container corroboration**: PST "Sent Items" contains messages
   to competitor-corp.com that are missing from mbox "Sent" —
   the mbox Sent folder has been partially deleted.
   - [GraphQuery: "PST-Sent to competitor-corp.com" → 47 messages]
   - [GraphQuery: "mbox-Sent to competitor-corp.com" → 0 messages]

## Thread Analysis
- The thread starting at [EmailMessage: msg-48291] shows cf@ negotiating
  compensation for "consulting services" with competitor-corp.com HR.
- Thread depth: 8 messages. Full exchange spans 2026-03-02 to 2026-04-15.

Conclusion: Evidence of unauthorized external communication and potential
data exfiltration. Recommend forensic review of the 23 matched-file emails
and the calendar meeting agendas.
```

3. Mark the entry as **Reviewed**. Create child **Action Items**:
   - "Export the 23 matched-file emails for external review"
   - "Extract calendar meeting descriptions from the 12 flagged events"
   - "Compare PST and mbox Sent folder hashes to confirm deletion"

---

## Quick Reference: Email Analysis Flow

```
Import PST + mbox → Folder tree (virtual email hierarchy)
      │
      ▼
Browse → Filter by sender/subject/date across both containers
      │
      ▼
Email Artifacts → Messages, Attachments, Calendar, Contacts
      │
      ▼
Correlate → EmailAttachment-to-FileName, EmailContact-to-Account
      │
      ▼
Thread Reconstruction → Graph query on In-Reply-To / Message-ID chain
      │
      ▼
Notebook → Document communication patterns with cross-container citations
```

---

*Walkthrough 02-Cross-Platform-Email version 1.0. Last updated: 2026-06-15.*
