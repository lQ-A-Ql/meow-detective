# image-webp 0.2.4

Upstream: https://github.com/image-rs/image-webp

Revision: `08b57d473c90148dd4b37a075df365afc3dfcc2b`

Crates.io archive SHA-256:
`525e9ff3e1a4be2fbea1fdf0e98686a6d98b4d8f937e1bf7402245af1909e8c3`

License: MIT OR Apache-2.0; this application selects MIT and retains both files.

The native RIFF chunk reader and chunk identifiers are exposed through
`inspection`. Existing VP8/VP8L dimension-reading code is shared by the full
decoder and metadata inspection. VP8X dimensions use the existing extended
header reader. No raster decoding is required by inspection.

RIFF odd-length rounding uses u64 arithmetic. The full decoder's extended
container scan ends at the declared RIFF end. VP8L dimensions mask the stored
14-bit value before adding one, retaining the valid 16384 dimension boundary.

The application bounds source reads, chunk counts and EXIF blocks and validates
every chunk range before publishing the container end.
