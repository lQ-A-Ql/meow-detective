# Linux MD native fixtures

Generated on 2026-11-26 with Linux 6.6.114.1 and mdadm 4.6.
Regenerate with `bash scripts/fixtures/generate-md-native.sh` as root on Linux.
The generator uses `.codex-tmp/md-native-fixtures` and releases arrays and loop
devices on exit; copy the reviewed compressed images and oracle into this folder.
Each compressed member expands to a 16 MiB image created with a loop device.
No private evidence is included. `oracle.bin` contains 256 KiB written to and
read back from each native MD device before the array was stopped.

Every array was created with `--run --assume-clean --force --size=8192`.
Striped arrays use `--chunk=4`. The fixture directories identify level and
metadata version. RAID5 uses left-symmetric; RAID6 uses parity-last; RAID10
uses n2. RAID0 and linear use the native component capacity.

The member images include the clean-stop metadata from mdadm. Tests compare
production discovery, logical capacity, complete oracle bytes, and missing-member
unaligned ranges. Corruption tests modify independent copies.

The retained 0.90 array has UUID `f87ce10e:9dbe4038:a0fc30d3:7420593a`,
events `4`, and component size `8192 KiB` according to `mdadm --examine`.
Regenerating metadata creates new UUIDs and requires reviewing this oracle.

The fixture bytes and oracle are generated test data, licensed under MIT.
