# ico 0.6.0

Upstream: https://github.com/mdsteele/rust-ico

Revision: `436291282e1a23954b69fcfe65d87c39928e76fa`

Crates.io archive SHA-256:
`d75e94d66081e4d8dcd098bd8dc5254800fea14a26389b298410158d834de167`

License: MIT. Upstream source, copyright, license, examples and tests are retained.

The existing directory-reading code is shared by `IconDir::read` and the new
`IconDir::read_metadata`. The metadata entry point bounds the entry count before
allocation and returns declared dimensions and source spans without allocating
or decoding image payloads. Normal full-file reads keep native span validation
before allocating payloads. The application owns source-length and range policy.
