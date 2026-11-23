# Image structure fixtures

Generated from constant-color images using ImageMagick 7.1.2-23 and its libwebp
delegate. No external images or evidence files are used. The fixtures are
distributed under the repository's MIT license.

- `animated.webp`: two 2x3 frames, red then blue, with 70 ms delay and infinite
  repetition. SHA-256:
  `30ddd8807940d730f96f3961165b1ce2f53da149e2ce2766a8bda89e1b58c621`.
- `lossy.webp`: one 2x2 red image encoded at quality75. SHA-256:
  `8557208a3d4805be45ebaaeb04c5e0b34dd2e9209972e4a63fae8e9e60d54ed9`.
- `baseline.jpg`: 2x3 red-to-blue gradient at quality90, sequential encoding.
  SHA-256: `a5f21f2244df526135aa242c5de9419b556ff1dc17ff7ca4c1c288ac3b05a36c`.
- `progressive.jpg`: the same gradient using progressive JPEG encoding.
  SHA-256: `96f7ee47a438cf61715f049529db8d2f8695ca120d7c501dd7df131c30902a0f`.

Regenerate from this directory with a workspace-controlled `TMPDIR`:

```sh
convert -delay 7 -size 2x3 xc:red xc:blue -loop 0 -define webp:lossless=true animated.webp
convert -size 2x2 xc:red -quality 75 -define webp:lossless=false lossy.webp
identify -format '%n %wx%h\n' animated.webp lossy.webp
convert -size 2x3 gradient:red-blue -quality 90 -interlace None baseline.jpg
convert -size 2x3 gradient:red-blue -quality 90 -interlace Plane progressive.jpg
```

JPEG metadata variations use img-parts0.4.2's native writer (MIT selected from
MIT OR Apache-2.0). EXIF TIFF payloads use the native kamadak-exif writer.
