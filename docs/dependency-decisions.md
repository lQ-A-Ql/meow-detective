# Dependency Decisions

This document records non-trivial dependency license and sourcing decisions for the Forensics Workbench workspace. Each entry explains why a license was added to the `deny.toml` allow-list or why a dependency was accepted despite an advisory/ban.

## License decisions

### CDLA-Permissive-2.0

- **Date**: 2026-06-21
- **Owner**: Registry Quality Maintenance
- **Crates**: `webpki-root-certs`, `webpki-roots`
- **License**: CDLA-Permissive-2.0
- **Decision**: Allowed
- **Reason**: These crates ship Mozilla/webpki root certificate data as a data bundle. The CDLA-Permissive-2.0 license is a data-specific permissive license that permits unrestricted use, modification, and distribution. The data is not executable code and does not impose copyleft or network-use obligations. It is pulled transitively by `rustls` / `tauri` networking stacks.
- **Review cadence**: Re-evaluate on each monthly dependency audit or when the root-cert data crate license changes.
