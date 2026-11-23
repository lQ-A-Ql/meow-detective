# kamadak-exif 0.6.1

Upstream: https://github.com/kamadak/exif-rs

Revision: `ba531e6bb523bf7c01849cf8e679beff4ef960b0`

Crates.io archive SHA-256:
`1130d80c7374efad55a117d715a3af9368f0fa7a2c54573afc15a188cd984837`

License: BSD-2-Clause; upstream copyright, license, source, examples and tests
are retained.

Local changes expose configurable directory, entry and field-value limits in
the existing TIFF parser, and the end of metadata storage visited by that parser.
Limits are checked before directory entries or field values are allocated.
TIFF raster ranges are derived by the application from native decoded tags.
EXIF and TIFF share this parser; the application contains no TIFF byte decoder.
Deprecated integer constants and elided return-type lifetimes use their current
Rust spelling; decoding semantics are unchanged by these syntax updates.

Affected application tests live in
`crates/evidence-access/tests/unit/file_service/image_structure/`.
