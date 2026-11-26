# Native LVM2 fixtures

The two `label_sectors` images were generated with LVM2 tools and are retained
from `lamco-admin/lamlvm` commit `578451ab0c106f884956afdfd1987451a88a406d`.
They exercise distinct PV UUIDs at identical source offsets. Each PV label is
stored in a different permitted sector.

The upstream project licenses these fixtures under MIT. Copyright 2022 main--
and 2026 Lamco Development LLC. The complete upstream LICENSE-MIT is retained.
