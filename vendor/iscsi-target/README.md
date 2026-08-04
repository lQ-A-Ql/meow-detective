# iscsi-target

A pure Rust iSCSI target implementation for building custom storage solutions.

## Overview

`iscsi-target` is a library that provides a reusable iSCSI target server. Users implement the `ScsiBlockDevice` trait to provide their own storage backend (in-memory, file-based, network-attached, etc.), and the library handles all iSCSI and SCSI protocol details.

## Features

- **Full iSCSI Protocol Support**: Login, logout, discovery sessions, normal sessions
- **SCSI Block Commands**: READ(6/10/16), WRITE(6/10/16), INQUIRY, READ CAPACITY(10/16), TEST UNIT READY, MODE SENSE, SYNCHRONIZE CACHE, REQUEST SENSE, REPORT LUNS, VERIFY
- **CHAP Authentication**: One-way and mutual CHAP support for secure connections
- **Discovery Sessions**: SendTargets protocol for target discovery
- **Write Operations**: Immediate data and multi-PDU Data-Out support
- **Real-World Tested**: Verified with Linux open-iscsi and iscsiadm, including CRC32C header/data digest interop
- **RFC-Conformance Tested**: Corpus-driven tests generated from an independent Prolog model of RFC 3720, with an explicit registry of every known deviation from the spec
- **Builder Pattern API**: Easy configuration and setup

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
iscsi-target = "1.0.0"
```

Create a simple in-memory target:

```rust
use iscsi_target::{IscsiTarget, ScsiBlockDevice, ScsiResult};

struct MemoryStorage {
    data: Vec<u8>,
}

impl ScsiBlockDevice for MemoryStorage {
    fn read(&self, lba: u64, blocks: u32, block_size: u32) -> ScsiResult<Vec<u8>> {
        let offset = (lba * block_size as u64) as usize;
        let len = (blocks * block_size) as usize;
        Ok(self.data[offset..offset + len].to_vec())
    }

    fn write(&mut self, lba: u64, data: &[u8], block_size: u32) -> ScsiResult<()> {
        let offset = (lba * block_size as u64) as usize;
        self.data[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn capacity(&self) -> u64 {
        (self.data.len() / 512) as u64
    }

    fn block_size(&self) -> u32 {
        512
    }

    fn flush(&mut self) -> ScsiResult<()> {
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let storage = MemoryStorage {
        data: vec![0u8; 100 * 1024 * 1024], // 100 MB
    };

    let target = IscsiTarget::builder()
        .bind_addr("0.0.0.0:3260")
        .target_name("iqn.2025-12.local:storage.disk1")
        .build(storage)?;

    target.run()?;
    Ok(())
}
```

## Connecting from Linux

```bash
# Discover targets
sudo iscsiadm -m discovery -t sendtargets -p 127.0.0.1:3260

# Login to target
sudo iscsiadm -m node -T iqn.2025-12.local:storage.disk1 -p 127.0.0.1:3260 --login

# Find device
lsblk

# Use the device (e.g., /dev/sdb)
sudo mkfs.ext4 /dev/sdb
sudo mount /dev/sdb /mnt/iscsi
```

## Examples

The `examples/` directory contains several examples:

- **simple_target.rs** - Basic in-memory storage target
- **chap_target.rs** - One-way CHAP authentication example
- **mutual_chap_target.rs** - Mutual CHAP authentication example
- **discover_targets.rs** - Discovery session client
- **graceful_shutdown.rs** - Handling shutdown signals
- **inspect_pdu_serialization.rs** - PDU serialization debugging tool

Run an example:

```bash
cargo run --example simple_target
```

## CHAP Authentication

Enable CHAP authentication for secure connections:

```rust
use iscsi_target::{IscsiTarget, AuthConfig};

let auth = AuthConfig::new_oneway_chap("username", "password123");

let target = IscsiTarget::builder()
    .bind_addr("0.0.0.0:3260")
    .target_name("iqn.2025-12.local:storage.secure-disk")
    .auth_config(Some(auth))
    .build(storage)?;
```

## Supported SCSI Commands

- INQUIRY (Standard and VPD pages)
- READ CAPACITY (10/16)
- READ (6/10/16)
- WRITE (6/10/16)
- TEST UNIT READY
- MODE SENSE (6/10)
- REQUEST SENSE
- REPORT LUNS
- SYNCHRONIZE CACHE (10/16)
- START STOP UNIT
- VERIFY

## Documentation

- [API Documentation](docs/README.md)
- [Implementation Guide](docs/IMPLEMENTATION.md)
- [CHAP Authentication](docs/CHAP_IMPLEMENTATION.md)
- [RFC 3720 Conformance Model](model/README.md)

## Testing

Run unit tests:

```bash
cargo test --lib
```

Run integration tests:

```bash
cargo test
```

### Conformance testing

How do you know this actually speaks RFC 3720? The protocol's logical rules —
key negotiation result-functions, the login state machine, sequence-number
windows, CHAP ordering — are modelled independently in Scryer Prolog, **derived
from the RFC rather than from this code**. The model generates a test corpus
(committed, so `cargo test` needs no Prolog) that is replayed against the real
implementation, and every place the code knowingly diverges from the spec is
recorded in an explicit deviation registry instead of being silently baked into
the tests. The model has caught real bugs the example-based tests missed,
including a result-function error masked at the default values.

See [model/README.md](model/README.md) for the model, the deviation registry,
and how to regenerate the corpus.

On top of the conformance corpus, digest support (CRC32C header and data
digests) has been interop-verified against a real Linux open-iscsi initiator,
including RFC 3720 §5.2.2 preference-order selection for list offers.

## Requirements

- Rust 1.82 or later (2021 edition)
- Linux (for testing with open-iscsi)
- Standard iSCSI port 3260 (or use custom port)

## Dependencies

- `byteorder` - Binary protocol parsing
- `thiserror` - Error handling
- `log` - Logging framework
- `md5` - CHAP authentication
- `rand` - Challenge generation
- `hex` - Hex encoding

## Project Status

Current version: 1.0.0

- Core iSCSI protocol: Complete
- SCSI commands: Complete for basic operations
- Write operations: Complete with immediate data and multi-PDU support
- CHAP authentication: Complete (one-way and mutual)
- Discovery sessions: Complete
- Real-world testing: Verified with Linux initiators

## License

Licensed under either of:

- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

## Contributing

Contributions are welcome! Please see the documentation for architectural details and implementation guidelines.

## Repository

https://github.com/lawless-m/iscsi-crate

## Author

Matt Lawless
