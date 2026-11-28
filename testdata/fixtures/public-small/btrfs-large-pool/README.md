# Native Btrfs device pool with 4097 files

`scripts/fixtures/generate-btrfs-large-pool.sh` creates two 256 MiB devices with
native `mkfs.btrfs`, mounts the complete device set, and writes 4097 authored
nonempty files through the Linux Btrfs driver. Data, metadata and system
profiles are SINGLE.

The tree comes from `generate-btrfs-large.sh`. Every filename and file hash is
compared against that independently restored single-device fixture. The file
named `evidence/entry-NNNNN.txt` contains the UTF-8 bytes produced by
`printf 'Btrfs namespace evidence %05d\n' N`, for N from 0 through 4096.

`files.sha256` records the complete file oracle while the pool is mounted.
`images.sha256` identifies both uncompressed members after unmounting.
`btrfs check --readonly`, native superblock observations and exact tooling
versions are retained with the fixture. Both devices must be supplied.

The fixture supports real collection import, namespace paging, selected file
reads and per-file verification beyond the active-work limit of 4096.
