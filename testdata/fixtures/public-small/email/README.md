# public-small email fixtures

Synthetic EML/EMLX/MBOX samples for the email artifact extractor regression gate.

## Samples

| File | Type | Purpose |
|------|------|---------|
| `plain.eml` | RFC 5322 single-part | Plain text body, minimal headers |
| `multipart.eml` | MIME multipart/mixed | multipart/alternative plain+HTML, attachment, Cc/Bcc/Reply-To/References |
| `encoded.eml` | RFC 2047 encoded headers | UTF-8 encoded `From` display name and `Subject` |
| `apple.emlx` | Apple Mail EMLX | Leading byte-count line, X-Mailer, X-Originating-IP |
| `simple.mbox` | RFC 4155 mbox | 2 plain text messages, reply threading |
| `multipart.mbox` | RFC 4155 mbox | multipart/alternative plain+HTML, attachment |
| `mboxrd_escaped.mbox` | mboxrd | Body contains `>From ` escaped line restored to `From ` |

## Visibility

public-small

## Source

Hand-crafted synthetic samples. No personal data.

## Expected JSON

`expected.json` — see the same directory.

## Coverage

- RFC 5322 header parsing (From/To/Cc/Bcc/Reply-To/Return-Path/Subject/Date/Message-ID/In-Reply-To/References/Received)
- MIME multipart/alternative and multipart/mixed
- Base64, quoted-printable and 7bit content transfer encodings
- Attachment filename, MIME type and byte-size extraction with `attachmentCount`
- RFC 2047 encoded word decoding
- Apple Mail EMLX byte-count line stripping
- X-Mailer, X-Originating-IP and X-Message-Class extraction (`xMailer`, `xOriginatingIp`, `messageClass`)
- Body preview generation (`bodyPreview`)
- EML/MBOX `isDeleted` metadata (single EML/MBOX messages are not deleted by construction)
- MBOX RFC 4155 variant detection (mboxrd/mboxo/mboxcl/mboxcl2)
- MBOX `>From ` unescaping
- MBOX container path attribution (`containerPath`)
- PST/OST message extraction with `messageClass` and deleted-folder heuristic (`isDeleted`)
- PST/OST size gate and best-effort encryption detection (encrypted or oversized containers are skipped)
- Per-message Artifact + Timeline event generation from a single mbox/PST/OST file

## Not guaranteed

- HTML sanitization (raw HTML body is preserved)
- S/MIME or PGP signed/encrypted messages
- TNEF/winmail.dat decoding
- Deep nested message/rfc822 part recovery
- Damaged/malformed MIME recovery beyond the simple top-level attachment fallback
- MBOX `Content-Length` delimited variants beyond heuristic detection
- Password-protected or encrypted PST/OST containers
