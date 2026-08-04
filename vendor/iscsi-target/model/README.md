# Scryer Prolog model of the iSCSI protocol

This directory contains a small **executable model** of the parts of iSCSI
whose behaviour is *logical* rather than wire-format. The rules are derived
from **RFC 3720** (and RFC 1994 for CHAP, RFC 1982 for serial-number
arithmetic) — **not** from the Rust implementation. The model is the source of
truth for a generated test corpus that is replayed against the real code by
`tests/model_corpus_tests.rs`.

The point: write the protocol rules **once, declaratively, from the spec**,
let Prolog enumerate cases exhaustively, and use the result as an *independent*
oracle. A model that merely restates the code is a tautology — so each
negotiation rule cites its RFC section, and every place the implementation
diverges from the RFC is recorded in an explicit `deviation/5` registry rather
than silently baked in. When the code and the RFC disagree, the corpus says so
out loud.

## Audited deviations from RFC 3720

These are the points where the current implementation diverges from the spec.
The corpus tags affected rows `status=deviation:Dn`, and the tests assert each
divergence is genuinely present; the registry is kept exhaustive by
`model_deviation_registry_is_exhaustive`.

| ID | Subject | RFC says | Implementation does | Status |
|----|---------|----------|---------------------|--------|
| **D1** | HeaderDigest / DataDigest | negotiable list `{None, CRC32C}` | ~~forced to `None`~~ → now negotiated and plumbed through | **fixed** |
| **D2** | FirstBurstLength | MUST NOT exceed MaxBurstLength | ~~constraint not enforced~~ → now clamped | **fixed** |
| **D3** | DataPDUInOrder / DataSequenceInOrder | boolean result function **OR** | ~~took initiator value directly~~ → now OR | **fixed** |
| **D4** | InitialR2T default | default is **Yes** | default is **No** | **open** — deliberate |

D1, D2 and D3 have been fixed (RFC 3720 §12.1, §12.14 and §12.18–12.19); the
model rows for them are now `conform`. For D1, digests are negotiated
(`src/session.rs negotiate_digest`: CRC32C whenever the initiator offers it,
else None) and plumbed through the wire path with the tgt/open-iscsi-correct
format: the digest is emitted little-endian (verified against fujita/tgt
`usr/iscsi/iscsid.c`, which writes the raw u32 in native order — little-endian
on the x86 hosts iSCSI runs on, the well-known byte-order divergence), and the
header digest covers BHS **+ AHS** with the correct
`BHS | AHS | HeaderDigest | Data | DataDigest` framing (`iscsi_digest`, pinned
by `target::tests::iscsi_digest_matches_tgt_wire_format` and
`header_digest_covers_bhs_and_ahs`). The one remaining open deviation is
documented rather than a bug:

- **D4** (RFC §12.10, `src/session.rs:119`): defaulting `InitialR2T=No` enables
  unsolicited/immediate data — a deliberate, widely-interoperable optimisation.
  The value is always explicitly declared in the login response, so the
  negotiation is RFC-conformant despite the differing default.

## Files

| File | Purpose |
|------|---------|
| `iscsi_protocol.pl` | The model: login state machine, key negotiation, sequence-number window, CHAP ordering. |
| `Makefile` | `make corpus` regenerates the corpus; `make check` verifies the committed corpus is up to date. |
| `../tests/corpus/iscsi_model_corpus.txt` | Generated corpus (committed, so `cargo test` needs no Prolog). |
| `../tests/model_corpus_tests.rs` | Rust test that replays the corpus against the implementation. |

## What is modelled

1. **Key negotiation result-functions** (`rfc_combine/4` vs `impl_combine/4`) —
   RFC 3720 §12. For each key the model computes both the RFC-correct result of
   combining the target's value with the initiator's offer *and* what the code
   actually does, then marks the row conform or deviation. The test sets the
   target value explicitly so the *result function* is exercised, not just the
   behaviour at the default. Minimum/Maximum for numerics, AND/OR for booleans,
   list-selection for digests — each citing its section (12.1, 12.10–12.20).
2. **Cross-key constraint** (`constraint_case/5`) — RFC 3720 §12.14,
   FirstBurstLength ≤ MaxBurstLength (deviation D2).
3. **Default-value conformance** (`default_case/4`) — RFC 3720 §12, the
   assumed-if-absent default of each key (deviation D4 on InitialR2T).
4. **Login state machine** (`next_state/4`) — RFC 3720 §5.3. A single login step
   from `Free` as a function of CSG / NSG / Transit per the RFC stage codes,
   checked against `AnySession::process_login`. All 32 `(CSG,NSG,Transit)`
   combinations are enumerated, including the illegal CSG values that must land
   in `Failed` — this guards the auth-bypass surface.
5. **Command-sequence-number window** (`sn_in_window/3`) — RFC 3720 §3.2.2.1 /
   RFC 1982. 32-bit serial-number arithmetic including wrap-around.
6. **CHAP message ordering** (`chap_case/4`) — RFC 3720 §11 / RFC 1994. The
   legal ordering of `AuthMethod`/`CHAP_A`/`CHAP_I`/`CHAP_C`/`CHAP_N`/`CHAP_R`
   and the coarse outcome of the first initiator message set.

## What is **not** modelled

- PDU byte encoding / BHS layout, digests, padding (`src/pdu.rs`) — serialization.
- SCSI block commands and the storage backend.
- Concurrency / async socket behaviour.

These are I/O and serialization concerns already covered by the crate's own
Rust tests; Prolog adds little there.

## Workflow

```bash
# One-time: install the interpreter
cargo install scryer-prolog

# Regenerate the corpus after changing the model
make -C model corpus

# CI / pre-commit: fail if the committed corpus drifted from the model
make -C model check

# Run the conformance tests (no Prolog needed; reads the committed corpus)
cargo test --test model_corpus_tests
```

## How to extend

To add coverage, add cases/rules to `iscsi_protocol.pl`, run `make -C model
corpus`, and (if a new record `KIND` was introduced) add a matching arm in
`tests/model_corpus_tests.rs`. The corpus format is one record per line:

```
SEQWINDOW  exp=<u32> max=<u32> sn=<u32> expect=<accept|reject>
NEGOTIATE  key=<Key> target=<value> init=<value> rfc=<value> impl=<value> section=<12.x> status=<conform|deviation:Dn>
CONSTRAINT name=<name> max_burst_offer=<u32> first_burst_offer=<u32> expect_max=<u32> expect_first=<u32> rfc=<valid|invalid> status=<conform|deviation:Dn>
DEFAULT    key=<Key> impl_default=<value> rfc_default=<value> section=<12.x> status=<conform|deviation:Dn>
STATE      csg=<0-3> nsg=<0-3> transit=<0|1> expect=<StateName>
CHAP       case=<name> params=<K:V,...|-> expect_state=<State> expect_key=<k=v|key|none>
```

Rows carrying `status=deviation:Dn` reference the deviation table above; the
`rfc` and `impl` columns differ on exactly those rows.

## Is this worth it?

Because the rules are derived from the RFC rather than the code, the model is
an **independent auditor**, not a mirror. That is what let it surface D1–D4 —
including D2 (a real conformance gap) and D3 (a result-function bug masked at
the default) — which example-based tests written alongside the implementation
would not catch, since they tend to encode the same reading of the spec the
code already has.

It earns its keep most on the **negotiation rules and login state machine** —
the min/max/AND/OR matrix and the 32 `(CSG,NSG,Transit)` transitions are
tedious and error-prone to maintain by hand, and the illegal-transition cases
are security-relevant. It is deliberately *not* a full formal model (no
TLA+-style temporal properties, no multi-step session traces yet) — it is a
lightweight, regenerable oracle that lives next to the code and keeps an honest
ledger of where the code and the spec part ways.
