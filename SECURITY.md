# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes |

## Reporting a Vulnerability

If you discover a security vulnerability in this project, please report it responsibly:

1. **Do not** open a public GitHub issue.
2. Email the maintainer with a description of the vulnerability.
3. Include steps to reproduce if possible.
4. Allow up to 7 days for an initial response.

## Security Considerations

- This application processes forensic evidence files. It does **not** execute or modify evidence.
- All file access is read-only by default.
- SQLite databases are local-only with no remote access.
- The Tauri Content Security Policy restricts network access to `self` and the `evidence-media:` custom protocol.
- No telemetry or analytics are collected.
