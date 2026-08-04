# iSCSI Target Implementation Guide

This document provides technical implementation details for completing the iSCSI target.

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                  IscsiTarget                        │
│                                                     │
│  ┌──────────────────────────────────────────────┐  │
│  │         TcpListener (port 3260)              │  │
│  └────────────────┬─────────────────────────────┘  │
│                   │                                 │
│                   ▼                                 │
│  ┌──────────────────────────────────────────────┐  │
│  │      IscsiConnection (per initiator)         │  │
│  │  - Login phase handler                       │  │
│  │  - Full feature phase handler                │  │
│  │  - PDU reader/writer                         │  │
│  └────────────────┬─────────────────────────────┘  │
│                   │                                 │
│                   ▼                                 │
│  ┌──────────────────────────────────────────────┐  │
│  │         IscsiSession (per session)           │  │
│  │  - Session parameters                        │  │
│  │  - Command sequence tracking                 │  │
│  │  - LUN mapping                               │  │
│  └────────────────┬─────────────────────────────┘  │
│                   │                                 │
│                   ▼                                 │
│  ┌──────────────────────────────────────────────┐  │
│  │         ScsiCommandHandler                   │  │
│  │  - Parse SCSI CDB                            │  │
│  │  - Call ScsiBlockDevice                      │  │
│  │  - Generate SCSI response                    │  │
│  └────────────────┬─────────────────────────────┘  │
│                   │                                 │
└───────────────────┼─────────────────────────────────┘
                    │
                    ▼
        ┌───────────────────────┐
        │   ScsiBlockDevice     │ ← User implements this
        │   (user's storage)    │
        └───────────────────────┘
```

## PDU Format (RFC 3720 Section 10)

Every iSCSI PDU has a 48-byte Basic Header Segment (BHS):

```
Byte/     0       |       1       |       2       |       3       |
    /              |               |               |               |
   |0 1 2 3 4 5 6 7|0 1 2 3 4 5 6 7|0 1 2 3 4 5 6 7|0 1 2 3 4 5 6 7|
   +---------------+---------------+---------------+---------------+
  0|.|I| Opcode    |F|  Opcode-specific fields                     |
   +---------------+---------------+---------------+---------------+
  4|TotalAHSLength | DataSegmentLength                             |
   +---------------+---------------+---------------+---------------+
  8| LUN or Opcode-specific fields                                 |
   +                                                               +
 12|                                                               |
   +---------------+---------------+---------------+---------------+
 16| Initiator Task Tag                                            |
   +---------------+---------------+---------------+---------------+
 20| Opcode-specific fields                                        |
   +                                                               +
 24|                                                               |
   +                                                               +
 28|                                                               |
   +                                                               +
 32|                                                               |
   +                                                               +
 36|                                                               |
   +                                                               +
 40|                                                               |
   +                                                               +
 44|                                                               |
   +---------------+---------------+---------------+---------------+
 48| Header-Digest (optional)                                      |
   +---------------+---------------+---------------+---------------+
 52| (DataSegment, optional)                                       |
   +---------------+---------------+---------------+---------------+
```

### Key Fields:

- **Opcode** (byte 0): PDU type
- **Flags** (byte 1): Command-specific flags
- **DataSegmentLength** (bytes 5-7): Length of data payload
- **LUN** (bytes 8-15): Logical Unit Number (for SCSI commands)
- **Initiator Task Tag** (bytes 16-19): Unique command identifier

### Common Opcodes:

```rust
pub const SCSI_COMMAND: u8 = 0x01;
pub const SCSI_RESPONSE: u8 = 0x21;
pub const SCSI_DATA_OUT: u8 = 0x05;
pub const SCSI_DATA_IN: u8 = 0x25;
pub const LOGIN_REQUEST: u8 = 0x03;
pub const LOGIN_RESPONSE: u8 = 0x23;
pub const TEXT_REQUEST: u8 = 0x04;
pub const TEXT_RESPONSE: u8 = 0x24;
pub const LOGOUT_REQUEST: u8 = 0x06;
pub const LOGOUT_RESPONSE: u8 = 0x26;
pub const NOP_OUT: u8 = 0x00;
pub const NOP_IN: u8 = 0x20;
```

## Login Phase Flow

```
Initiator                                Target
    |                                        |
    |  Login Request (SecurityNegotiation)   |
    |--------------------------------------->|
    |                                        | Check auth method
    |  Login Response (SecurityNegotiation)  |
    |<---------------------------------------|
    |                                        |
    |  Login Request (LoginOperationalNeg)   |
    |--------------------------------------->|
    |                                        | Negotiate params
    |  Login Response (LoginOperationalNeg)  |
    |<---------------------------------------|
    |                                        |
    |  Login Request (FullFeaturePhase)      |
    |--------------------------------------->|
    |                                        | Create session
    |  Login Response (FullFeaturePhase)     |
    |<---------------------------------------|
    |                                        |
    |        Full Feature Phase              |
    |<======================================>|
```

### Login Parameters to Negotiate:

```
InitiatorName=iqn.2025-12.local:initiator
TargetName=iqn.2025-12.local:storage.disk1
SessionType=Normal
HeaderDigest=None
DataDigest=None
MaxRecvDataSegmentLength=8192
MaxBurstLength=262144
FirstBurstLength=65536
DefaultTime2Wait=2
DefaultTime2Retain=20
MaxOutstandingR2T=1
DataPDUInOrder=Yes
DataSequenceInOrder=Yes
ErrorRecoveryLevel=0
```

## SCSI Command Flow

```
Initiator                                Target
    |                                        |
    |  SCSI Command (READ 10)                |
    |--------------------------------------->|
    |                                        | Parse CDB
    |                                        | Call device.read()
    |  SCSI Data-In (data payload)           |
    |<---------------------------------------|
    |                                        |
    |  SCSI Response (status)                |
    |<---------------------------------------|
```

### SCSI Command PDU Structure:

```
Byte 0: 0x01 (SCSI Command)
Byte 1: Flags (0x80 = Final, 0x40 = Read, 0x20 = Write)
Bytes 8-15: LUN
Bytes 16-19: Initiator Task Tag
Bytes 20-23: Expected Data Transfer Length
Bytes 32-47: CDB (Command Descriptor Block)
```

### Common SCSI CDBs:

**INQUIRY (0x12):**
```
Byte 0: 0x12 (INQUIRY)
Byte 1: 0x00
Byte 2: 0x00 (page code)
Byte 3-4: Allocation length (typically 96 bytes)
```

**READ CAPACITY 10 (0x25):**
```
Byte 0: 0x25 (READ CAPACITY 10)
Bytes 1-9: 0x00
```

**READ 10 (0x28):**
```
Byte 0: 0x28 (READ 10)
Byte 1: 0x00
Bytes 2-5: LBA (logical block address)
Byte 6: 0x00
Bytes 7-8: Transfer length (number of blocks)
Byte 9: 0x00
```

**WRITE 10 (0x2A):**
```
Byte 0: 0x2A (WRITE 10)
Byte 1: 0x00
Bytes 2-5: LBA
Byte 6: 0x00
Bytes 7-8: Transfer length
Byte 9: 0x00
```

## Implementation Strategy

### Phase 1: PDU Parsing (src/pdu.rs)

```rust
pub struct IscsiPdu {
    pub opcode: u8,
    pub flags: u8,
    pub ahs_length: u8,
    pub data_length: u32,
    pub lun: u64,
    pub itt: u32,
    pub fields: [u8; 28],  // Opcode-specific
    pub data: Vec<u8>,
}

impl IscsiPdu {
    pub fn from_bytes(buf: &[u8]) -> Result<Self, IscsiError>;
    pub fn to_bytes(&self) -> Vec<u8>;
}
```

### Phase 2: Session Management (src/session.rs)

```rust
pub struct IscsiSession {
    pub isid: u64,
    pub tsih: u16,
    pub cmd_sn: u32,
    pub max_cmd_sn: u32,
    pub exp_stat_sn: u32,
    pub params: SessionParams,
    pub luns: HashMap<u64, Arc<Mutex<dyn ScsiBlockDevice>>>,
}

pub struct SessionParams {
    pub max_recv_data_segment_length: u32,
    pub max_burst_length: u32,
    pub first_burst_length: u32,
    pub max_outstanding_r2t: u32,
}
```

### Phase 3: SCSI Handlers (src/scsi.rs)

```rust
pub fn handle_scsi_command(
    cdb: &[u8],
    device: &mut dyn ScsiBlockDevice,
) -> Result<ScsiResponse, IscsiError> {
    match cdb[0] {
        0x12 => handle_inquiry(cdb, device),
        0x25 => handle_read_capacity_10(cdb, device),
        0x28 => handle_read_10(cdb, device),
        0x2A => handle_write_10(cdb, device),
        _ => Err(IscsiError::Scsi("Unsupported command".into())),
    }
}

pub struct ScsiResponse {
    pub status: u8,  // 0x00 = Good
    pub data: Vec<u8>,
    pub sense: Option<SenseData>,
}
```

### Phase 4: Target Server (src/target.rs)

```rust
impl<D: ScsiBlockDevice + Send + 'static> IscsiTarget<D> {
    pub fn run(self) -> ScsiResult<()> {
        let listener = TcpListener::bind(&self.bind_addr)?;

        for stream in listener.incoming() {
            let stream = stream?;
            let device = Arc::clone(&self.device);
            let target_name = self.target_name.clone();

            thread::spawn(move || {
                handle_connection(stream, device, target_name)
            });
        }
        Ok(())
    }
}

fn handle_connection<D: ScsiBlockDevice>(
    mut stream: TcpStream,
    device: Arc<Mutex<D>>,
    target_name: String,
) -> Result<(), IscsiError> {
    // Login phase
    let session = handle_login(&mut stream, &target_name)?;

    // Full feature phase
    loop {
        let pdu = read_pdu(&mut stream)?;

        match pdu.opcode {
            SCSI_COMMAND => {
                let response = handle_scsi_command(&pdu, &device)?;
                write_pdu(&mut stream, response)?;
            }
            LOGOUT_REQUEST => {
                write_pdu(&mut stream, logout_response())?;
                break;
            }
            _ => {}
        }
    }
    Ok(())
}
```

## Testing Strategy

### Unit Tests

Test each component in isolation:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_pdu_parse_login_request() {
        let bytes = vec![/* login request PDU */];
        let pdu = IscsiPdu::from_bytes(&bytes).unwrap();
        assert_eq!(pdu.opcode, 0x03);
    }

    #[test]
    fn test_scsi_inquiry() {
        let device = MockDevice::new();
        let cdb = [0x12, 0, 0, 0, 96, 0];
        let response = handle_inquiry(&cdb, &device).unwrap();
        assert_eq!(response.status, 0x00);
    }
}
```

### Integration Tests

Test with real iSCSI initiators:

```bash
# Linux
sudo iscsiadm -m discovery -t sendtargets -p 127.0.0.1
sudo iscsiadm -m node --login
lsblk  # Should see new disk

# Windows
iscsicli AddTargetPortal 127.0.0.1 3260
iscsicli ListTargets
iscsicli LoginTarget <target-name>
```

## Debugging Tips

### Use Wireshark

Filter: `iscsi`

This shows all iSCSI PDUs on the wire. Compare with working implementations.

### Enable Logging

```rust
env_logger::init();
log::debug!("Received PDU: opcode={:02x}", pdu.opcode);
```

### Compare with TGT

Run TGT alongside and compare packet captures to see differences.

## Performance Considerations

### Buffer Sizes

- Use `MaxRecvDataSegmentLength` negotiated value
- Typical: 8192 bytes (8 KB)
- Maximum: 16777215 bytes (~16 MB)

### Zero-Copy Reads

```rust
// Instead of allocating new Vec
device.read(lba, blocks, block_size)?

// Consider returning borrowed slices
device.read_into(lba, blocks, &mut buf)?
```

### Async I/O (Future Enhancement)

Consider tokio for async TCP and concurrent connections.

## Common Pitfalls

1. **Byte Order**: iSCSI is big-endian, use `byteorder` crate
2. **Padding**: Data segments padded to 4-byte alignment
3. **Sequence Numbers**: Must track CmdSN, StatSN carefully
4. **LUN Format**: 64-bit, usually just [0,0,0,0,0,0,0,LUN]
5. **Status**: SCSI status in SCSI Response, not iSCSI status

## References

Keep these open while implementing:

- [RFC 3720](https://datatracker.ietf.org/doc/html/rfc3720) - THE specification
- [SBC-4](https://www.t10.org/drafts.htm) - SCSI Block Commands
- [Wireshark iSCSI dissector source](https://gitlab.com/wireshark/wireshark/-/blob/master/epan/dissectors/packet-iscsi.c) - Reference implementation
