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

## Provenance

- Generator: `scripts/generate_medium_email_fixtures.py`
- Reproducibility: fixed timestamps, random seed, and Message-ID values
- License: repository MIT license
- Sensitivity review: synthetic identities and reserved example domains only; no personal data, credentials, or tokens

| File | Bytes | SHA-256 |
|------|------:|---------|
| `plain.eml` | 365 | `6d1d7375c0bb041929b912484d4ccf1f55e749be2caebc1f8151f0832cba88c8` |
| `html.eml` | 769 | `1ed3b6e0fc242c2b878e0655b55a8b03622614e2cd40a004828e43f5f66a3e25` |
| `attachment.eml` | 726 | `f6033ad5938a3a861f8061c57c85dbcb6e012131fb8f998a0347f84b43707b74` |
| `multipart_related.eml` | 989 | `119dbd68318f5fda326a2e91363da5a46f4e6f1e798e5242152d1880b1a012cd` |
| `encoded_headers.eml` | 351 | `666c694c5c94f3e3e4ed3f64fc3ca5a0925925b99a6218cd7eb3c6b7be459823` |
| `japanese.eml` | 380 | `f26664ccf30f8103e8aa8fd11b9bb92eaee46afb5470b3f09bf79e7aac982750` |
| `thread_root.eml` | 391 | `d0fde54743e7f6d649796de92eb4fd93eff76f73694234bfd30a3def2221e6c4` |
| `thread_reply.eml` | 480 | `c6e4a23fcee19f6ea22e95daca0967be8bb6cf9b18e165f3fc2feb3003ed3b38` |
| `x_headers.eml` | 348 | `d4211566bb08a73120b5b7c976962dcc8138a4470a14e2d20ae9d88f5742a388` |
| `quoted_printable.eml` | 367 | `d2eb26449f6082a7d15d7b40cd227ac72b7d6864eead8799052c150383bd7a8b` |
| `no_subject.eml` | 298 | `ac17c1bb3eb7c8f02397e771d6951884fb4c59992b3d9a5c4d4c7e1fd4690e32` |
| `long_subject.eml` | 446 | `dfa61f2579c1282e0543e9e7388d5bb564c5eb89160260c2cdae11df44891603` |
| `nested_rfc822.eml` | 873 | `f46f9735d086a235c97c0f861ac2421424a43f75673dda8ff9f427a40665b46e` |

## Expected JSON

`expected.json` in this directory.
