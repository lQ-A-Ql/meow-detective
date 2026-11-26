# Native multi-PV LVM / MD / EXT4 fixture

Generated on 2026-11-27 with LVM2, mdadm 4.6, and mke2fs/debugfs 1.47.4.
The retained `pvs.json`, `vgs.json`, and `md-detail.txt` are native tool output.
Regenerate with `bash scripts/fixtures/generate-lvm-md-native.sh` as root.

Each member expands to a 16 MiB PV. PV 0 has one metadata area and PV 1 has
none (`pvcreate --metadatacopies 0`). The VG has two 8 MiB linear LVs, one on
each PV; those LVs form a 14 MiB native MD RAID0 array with 4 KiB chunks.
An EXT4 filesystem contains `/evidence.bin` with the bytes from
`../../mdraid/oracle.bin`. `debugfs` and `cmp` verified the file before array
shutdown. The PV images were compressed after MD shutdown and VG deactivation,
before the generator removed its VG and detached its loop devices.

The generator uses `.codex-tmp/lvm-md-native` and a private LVM configuration.
It disables LVM backups and archives and releases its array, VG, and loops.
Generated fixture bytes and tool output are licensed under MIT.
