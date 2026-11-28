# Native Btrfs namespace with 4097 files

`scripts/fixtures/generate-btrfs-large.sh` creates 4097 authored, nonempty files
and uses `mkfs.btrfs --rootdir` to populate a real single-device Btrfs filesystem.
The data, metadata and system profiles are SINGLE. The uncompressed image is
256 MiB; the files use native inline extents.

The file named `evidence/entry-NNNNN.txt` contains the UTF-8 bytes produced by
`printf 'Btrfs namespace evidence %05d\n' N`, for N from 0 through 4096.
`files.sha256` records every original file. After filesystem creation,
`btrfs check --readonly` verifies the image and `btrfs restore` extracts every
file into a separate directory. `restored-oracle.txt` records the complete
successful comparison against the original file hashes.

`image.sha256` identifies the uncompressed image. `tool-versions.txt`,
`mkfs.txt`, `check.txt` and `super.txt` retain the native tooling and filesystem
observations. The fixture contains no acquired evidence.

Tests using this fixture can verify import, fixed snapshot pagination, a
selected file and complete file hashes independently of the application
parser. This fixture alone does not cover multi-device or degraded layouts.
