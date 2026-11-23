# jpeg-decoder 0.3.2

Upstream: https://github.com/image-rs/jpeg-decoder

Revision: `eb2d7c0f6a2d0298aba7a7f8b9ca1440353e8f8c`

Crates.io archive SHA-256:
`00810f1d8b74be64b13dbf3db89ac67740615d6c891f0e7b6179326533011a07`

License: MIT OR Apache-2.0; MIT is selected and both files are retained.

The existing native marker classification and SOF parser are exposed through a
metadata-only `frame_dimensions` wrapper. The full decoder and inspection use
the same frame-header grammar. Inspection never allocates pixel storage.
Default Rayon support is disabled for this application.
Worker target conditions use supported Rust target names; the wasm32 exclusion
is retained.
