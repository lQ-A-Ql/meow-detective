# public-medium EML fixtures

Synthetic EML samples covering common forensic email scenarios.

## Samples

| File | Purpose |
|------|---------|
| plain.eml | Single-part plain text |
| html.eml | multipart/alternative plain+HTML, Cc |
| attachment.eml | multipart/mixed with base64 attachment |
| multipart_related.eml | multipart/related with inline image |
| encoded_headers.eml | RFC 2047 encoded Chinese headers |
| japanese.eml | UTF-8 Japanese headers and body |
| thread_root.eml | Message-ID for threading |
| thread_reply.eml | In-Reply-To / References |
| x_headers.eml | X-Mailer and X-Originating-IP |
| quoted_printable.eml | Quoted-printable transfer encoding |
| no_subject.eml | Missing Subject header |
| long_subject.eml | Long folded Subject header |
| nested_rfc822.eml | message/rfc822 forward attachment |

## Visibility

public-medium

## Source

Synthetic samples. No personal data.

## Expected JSON

`expected.json` in this directory.
