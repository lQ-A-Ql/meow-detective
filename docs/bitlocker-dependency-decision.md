# BitLocker Dependency Decision

**Updated**: 2026-07-27
**Scope**: `crates/volume-bitlocker` — BitLocker (BDE) volume decryption layer
**Related**: `docs/bitlocker-volume-layer-design.md`

## Resolved Decision

`crates/volume-bitlocker` is a **derived work** of `bitlocker-core` from
`SecurityRonin/bitlocker-forensic`, vendored at a pinned commit rather than
consumed from crates.io.

Why vendored rather than a crates.io dependency:

- `deny.toml` sets `allow-git = []`, so a git dependency is not an option. The
  established precedent for non-registry source is vendoring (`crates/evtx-patched`).
- The upstream `bitlocker-core` public API returns its own `BdeError` and expects
  to own the volume handle. Our evidence layer requires errors mapped into
  `transport::errors::ApiErrorDto` classification and readers that compose with
  `PartitionWindowReader`. That is an adaptation, not a wrapper.
- Vendoring lets us drop the upstream `vfs` feature, its `forensic-vfs` dependency,
  and the `forensic` binary crate entirely — none of which we consume.
- The credential-boundary rules in `docs/bitlocker-volume-layer-design.md` §2.4
  (no `Debug`/`Clone`/`Serialize` on secret types, zeroize on drop) are stricter
  than upstream's and must be enforced in types we own.

## Upstream Provenance

| Fact | Value |
|------|-------|
| Repository | `https://github.com/SecurityRonin/bitlocker-forensic` |
| Commit | `7c931d4be338a172de9799476eb744ba089e0867` |
| Commit date | 2026-07-25T05:48:54Z |
| Commit subject | `chore(bitlocker-core): release v0.3.5` |
| Upstream version | `bitlocker-core` 0.3.5 |
| Root tree hash | `7aa92b65ca33dbe3d0fc8b32dde7852301e91e8a` |
| `core/` subtree hash | `2460c511a3418fb1c50c72edd2b935e6aee8e567` |
| `core/src/` tree hash | `b00579976427432de5cac3f24470d47c7fac90a7` |
| License | Apache-2.0 |
| Author | Albert Hui `<albert@securityronin.com>` |
| Upstream MSRV | 1.81 |

The Elephant Diffuser lives in a separate upstream crate, also vendored:

| Fact | Value |
|------|-------|
| Repository | `https://github.com/SecurityRonin/elephant-diffuser` |
| Commit | `09f029505f01e0ade7d986b6a2f3b2421453a02a` |
| `src/` tree hash | `be16c5da60fef735d7f198f452ffc79d56e1421d` |
| License | Apache-2.0 |
| Size | 220 lines (`src/lib.rs`) |

It is vendored rather than depended on because it has 614 total downloads from a
single author. At 220 lines the review cost is lower than the supply-chain cost
of an unreviewed low-adoption crate on an evidence-decryption path.

Vendored in Stage 2 as `crates/volume-bitlocker/src/diffuser.rs`, decrypt
direction only — production never encrypts, and the test-layout rules forbid
hiding an unused encrypt path behind `#[cfg(test)]` in `src`. The upstream
regression vector came across into the tests: it is the only check that can catch
a transposed rotation constant, because a round-trip moves both directions
together.

### Source checksums at the pinned commit

SHA-256 of each upstream file the derived work is based on. These are the
integrity anchors: a future upstream refresh must re-record them.

```
f0bd3b5d5a9a2409d09bb70d44ea0a965adc1db1b23662979e8f04c55c4b21cd  core/src/bytes.rs
09af5f3ee0f1943e4bcd8fcaf73ac4dcf6cbb6b76fe60a5d0300a539d41bfa3f  core/src/crypto.rs
cd74e250ed0094222b887eea4e4d403130eaef119558e4daca3a9d66aac54824  core/src/error.rs
497fdceec340d282c20b4f6c3190ee0b85c87e5936467efc4b46777233cd3eb3  core/src/guid.rs
09246a85334bfeb810831af33b9b5a6ccfffdf4b0eaaf65cd9ddfe41d07e089c  core/src/header.rs
2ef0bbad1baaf5f9ed0554a6bcde5abcda61409b61503c4e5e294fc761711c1c  core/src/lib.rs
3169510e65a0e4b6ef36fa9982818f3d2926b1d43f7b04a25ea5f42a7a5a138d  core/src/metadata.rs
3730745b8626d1ee03d82ac5ffb3b2b42299bdaa1b359b16ffca606dacc5b8a5  core/src/method.rs
e8ce8d4b8b6c1db15a37e8961f3ea462ba7b06f0e8b916e45b076f91fb8250e0  core/src/volume.rs
3ddf9be5c28fe27dad143a5dc76eea25222ad1dd68934a047064e56ed2fa40c5  LICENSE
36fc68864a5a4272a63bfea201473985fb59a138382547f90262bab3af429539  elephant-diffuser/src/lib.rs
```

Reproduce with:

```powershell
git clone https://github.com/SecurityRonin/bitlocker-forensic $tmp
git -C $tmp checkout 7c931d4be338a172de9799476eb744ba089e0867
Get-FileHash "$tmp\core\src\volume.rs" -Algorithm SHA256
```

## What Is Excluded

Not carried into `crates/volume-bitlocker`:

- The `forensic` binary crate and its `jiff` / `forensicnomicon` dependencies.
- The `vfs` feature and the `forensic-vfs` 0.7 dependency. Our evidence layer
  has its own reader composition; the upstream `EncryptionLayer` contract is a
  second, conflicting abstraction.
- Upstream `fuzz/` targets, `mkdocs` documentation, and `.github` workflows.
- Clear-key unlock. Upstream implements it; v1 only reports it as a protector
  (see `docs/bitlocker-volume-layer-design.md` §2.2 for why).
- Startup-key (`.BEK`) unlock.

## New Dependencies

Already in `[workspace.dependencies]` and unchanged: `aes ~0.8`, `cbc ~0.1`,
`sha2 ~0.10`, `zeroize ~1`, `thiserror ~2.0`.

Added by this decision:

| Crate | Version | License | Note |
|-------|---------|---------|------|
| `ccm` | `~0.5` | Apache-2.0 OR MIT | RustCrypto AES-CCM, for VMK/FVEK unwrap |
| `xts-mode` | `~0.5` | MIT | XTS-AES sector mode, methods `0x8004`/`0x8005` |

`ccm` is pinned to the `0.5` release line rather than the `0.6.0-rc` prerelease.

### Why `xts-mode ~0.5` adds no AES generation

Measured, not assumed. `xts-mode` 0.5.1 requires `cipher ^0.4`, which is the same
generation this workspace already pins through `aes ~0.8`. `cargo tree -p
volume-bitlocker` shows the whole crate resolving to `aes 0.8.4` and `cipher
0.4.4`, with `xts-mode 0.5.1` and the already-present `byteorder 1.5.0` as the
only additions.

`xts-mode` 0.6 would move to `cipher` 0.5 / `aes` 0.9. That is the version to
avoid, and not because 0.9 is absent: `Cargo.lock` already carries **both** `aes`
0.8.4 and 0.9.1. The 0.9 edge comes from `lopdf` 0.44 through `app-services` for
PDF decryption, entirely separate from the evidence-cipher path. Taking
`xts-mode` 0.6 would put a third crypto stack in the tree and split
`volume-bitlocker`'s own AES between two generations for no capability gain.

The pre-existing 0.8/0.9 duplication is out of scope here. `deny.toml` sets
`multiple-versions = "warn"`, so it needs no ban exception, and collapsing it
would mean changing the PDF path.

Both licenses are already in the `deny.toml` allow list, so no license exception
is required. No advisory exception is required.

### Accepted limitation: AES key-schedule residue

`aes` 0.8 exposes only a `hazmat` feature — there is no `zeroize` there. The FVEK
and tweak bytes in `VolumeKeyPackage` are wiped on drop, but the expanded key
schedules inside the `Aes128` / `Aes256` values held by `SectorCipher` are not.

`aes` 0.9 does offer key-schedule zeroization, and adopting it would break
`xts-mode` 0.5 as described above. The residue is therefore accepted for v1 and
bounded structurally instead: one `SectorCipher` exists per unlocked volume rather
than per read, so the number of live schedules is the number of unlocked volumes,
not the number of reads. Revisit when a maintained XTS crate tracks `cipher` 0.5.

## Attribution

Apache-2.0 requires retaining the license, attribution notices, and a statement
of modification. `crates/volume-bitlocker/` therefore carries:

- `LICENSE-APACHE-2.0-UPSTREAM` — the verbatim upstream license text.
- `NOTICE` — upstream copyright, the pinned commit, and a summary of the changes
  made in the derived work.

Upstream ships no `NOTICE` file of its own, so ours is the only one and must not
claim to reproduce one.

## Anti-Regression Guard

`scripts/check-bitlocker-credential-guard.ps1` enforces the credential boundary
and the provenance record together. It fails if:

- this decision record stops naming the pinned upstream commit;
- a secret-bearing type in `crates/volume-bitlocker` derives `Debug`, `Clone`,
  or `Serialize`;
- a credential value reaches a logging, formatting, or serialization sink;
- the crate creates plaintext temporary files or opens the evidence path writable;
- `unsafe_code` stops being forbidden.

## Follow-Up

1. Re-check upstream releases periodically; a maintained upstream that matches
   our error and reader contracts would let us drop the derived copy.
2. Keep the derived work minimal. Do not port the CLI, VFS layer, or fuzz targets.
3. Re-record the checksums above on any upstream refresh, in the same commit as
   the code change.
4. Stage 4 must decide the retention and expiry policy for stored FVEK key
   packages before any credential persistence ships.
