# tinybmp 0.7.0

Upstream: https://github.com/embedded-graphics/tinybmp
Revision: 4b307a814d5b97a08d9d9ca6d2b98c4778446ae5
Registry archive SHA-256: acb540c3e6f049f914d9c3eaf86ca415e264cf42d2b25bb5c24241af1a1e3aea
License: MIT OR Apache-2.0. This application selects MIT; both upstream license files are retained.

The shared native header parser exposes `Header::from_slice` for bounded metadata
inspection without palette/pixel reads. `RawBmp` and the header entry point use
the same parser. Color planes must equal one, indexed palette sizes must fit
their bit depth, and pixel data must follow the complete header and palette.
`Header::image_data_length` provides the shared padded-row extent calculation;
RGB and Bitfields use their uncompressed layout, including when `biSizeImage` is
zero. Compressed layouts retain their declared data length.

The original parser, image data, tests and license notices remain from upstream.
No parallel BMP parser is maintained by the application.
