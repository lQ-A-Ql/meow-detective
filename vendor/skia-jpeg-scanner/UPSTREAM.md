# Skia JPEG segment scanner

Upstream: https://github.com/google/skia

Revision: `5dac00a4477109740abe5c69150db2beb4379d8d`

Source: `experimental/rust_jpeg/ffi/jpeg_segment_scan.rs`

Source SHA-256:
`c68fe5460aa067ab1a80b49718438b36990c77b89189db447421580adef4ae8f`

License: BSD-3-Clause, retained in `LICENSE`.

This is Skia's upstream experimental Rust port of its production C++ segment
scanner. The source is unchanged; the local Cargo manifest packages that file.
The application owns byte/segment limits and checks source ranges before reads.
Frame-header semantics use the native jpeg-decoder parser through one facade.
