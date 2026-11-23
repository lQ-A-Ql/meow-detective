# gif 0.14.2

Upstream: https://github.com/image-rs/image-gif

Revision: `a0127e2acbef333398414bb61d097c19c1b09d8a`

Crates.io archive SHA-256:
`ee8cfcc411d9adbbaba82fb72661cc1bcca13e8bba98b364e62b2dba8f960159`

License: MIT OR Apache-2.0; this application selects MIT and retains both files.

The existing `StreamingDecoder::with_options` constructor is public. The
application uses native block/sub-block events, skips pixel decoding, bounds
reads and events, checks frame bounds, and keeps exact source ranges. The
native GIF grammar and decoding implementation are unchanged.
