# Native storage composition oracles

Generator: `scripts/fixtures/generate-storage-combinations.sh`.
All images contain authored content from `../mdraid/oracle.bin`.
The generator creates Linux MD RAID0 members using mdadm and populates the selected
filesystem through the Linux kernel. No acquired evidence is included.

| Directory | Native layout |
|---|---|
| `md-gpt-ext4` | MD RAID0, GPT partition at 1 MiB, EXT4 |
| `md-lvm-ext4` | MD RAID0, native LVM PV/VG/linear LV, EXT4 |
| `md-xfs` | MD RAID0, direct XFS |
| `md-btrfs` | MD RAID0, direct single-device Btrfs with DUP metadata |

`logical.sha256` records the complete MD logical device after filesystem unmount.
`files.sha256` records the file bytes read from the mounted native filesystem.
The fixtures retain native MD identity, partition metadata or LVM metadata as
applicable. XFS uses explicitly selected features supported by Linux 6.6; the
generator records their exact creation options and all tool versions.
Tests check the full logical digest, catalog attribution, FileEntryId preview,
complete file bytes, cross-stripe ranges, public work execution and foreign-case rejection.
