# Native Btrfs device pools

Generator: `scripts/fixtures/generate-btrfs-pools.sh`.
The generated images contain authored deterministic content and no acquired evidence.
`mkfs.btrfs` creates the filesystems; Linux Btrfs mounts populate the files, and
`sha256sum` records the content while each complete device set remains mounted.
`tool-versions.txt` records the native tooling and Linux kernel.

Admitted data/metadata profiles are SINGLE, RAID0, RAID1 and RAID10 with complete
device sets. Each member is 256 MiB before gzip compression. `stale-member.raw.gz`
is a genuine copy taken before the kernel writes the content transaction.
The current and stale member retain the same native filesystem and device IDs.

The native tests compare complete content and stripe/tail ranges with `oracle.bin`,
reverse input order, and reject missing devices, duplicate device IDs, foreign
filesystems and mixed generations. Actual data-extent corruption must fail its
native checksum after every copy of the selected content sector is damaged.
No degraded-device or parity-profile admission is implied.
