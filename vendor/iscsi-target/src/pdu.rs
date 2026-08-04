//! iSCSI PDU (Protocol Data Unit) parsing and serialization
//!
//! This module handles the binary protocol format for iSCSI PDUs
//! based on RFC 3720: https://datatracker.ietf.org/doc/html/rfc3720

// Protocol functions require many parameters per RFC 3720
#![allow(clippy::too_many_arguments)]

use crate::error::{IscsiError, ScsiResult};
use byteorder::{BigEndian, ByteOrder, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;

/// BHS (Basic Header Segment) size in bytes
pub const BHS_SIZE: usize = 48;

/// iSCSI PDU Opcodes (RFC 3720 Section 10)
pub mod opcode {
    // Initiator opcodes (client → target)
    pub const NOP_OUT: u8 = 0x00;
    pub const SCSI_COMMAND: u8 = 0x01;
    pub const TASK_MANAGEMENT_REQUEST: u8 = 0x02;
    pub const LOGIN_REQUEST: u8 = 0x03;
    pub const TEXT_REQUEST: u8 = 0x04;
    pub const SCSI_DATA_OUT: u8 = 0x05;
    pub const LOGOUT_REQUEST: u8 = 0x06;
    pub const SNACK_REQUEST: u8 = 0x10;

    // Target opcodes (target → client)
    pub const NOP_IN: u8 = 0x20;
    pub const SCSI_RESPONSE: u8 = 0x21;
    pub const TASK_MANAGEMENT_RESPONSE: u8 = 0x22;
    pub const LOGIN_RESPONSE: u8 = 0x23;
    pub const TEXT_RESPONSE: u8 = 0x24;
    pub const SCSI_DATA_IN: u8 = 0x25;
    pub const LOGOUT_RESPONSE: u8 = 0x26;
    pub const R2T: u8 = 0x31;
    pub const ASYNC_MESSAGE: u8 = 0x32;
    pub const REJECT: u8 = 0x3F;
}

/// iSCSI PDU flags (commonly used across PDU types)
pub mod flags {
    // Common flags
    pub const FINAL: u8 = 0x80;
    pub const CONTINUE: u8 = 0x40;

    // SCSI command flags
    pub const READ: u8 = 0x40;
    pub const WRITE: u8 = 0x20;

    // SCSI Data-In flags
    pub const DATA_IN_OVERFLOW: u8 = 0x04;
    pub const DATA_IN_UNDERFLOW: u8 = 0x02;
    pub const DATA_IN_STATUS: u8 = 0x01;

    // Login flags
    pub const TRANSIT: u8 = 0x80;
    pub const CONTINUE_LOGIN: u8 = 0x40;

    // Login stages (CSG/NSG in bits 2-3 and 0-1)
    pub const CSG_SECURITY_NEG: u8 = 0x00;
    pub const CSG_LOGIN_OP_NEG: u8 = 0x04;
    pub const CSG_FULL_FEATURE: u8 = 0x0C;
    pub const NSG_SECURITY_NEG: u8 = 0x00;
    pub const NSG_LOGIN_OP_NEG: u8 = 0x01;
    pub const NSG_FULL_FEATURE: u8 = 0x03;
}

/// Login status classes (RFC 3720 Section 10.13.5)
pub mod login_status {
    pub const SUCCESS: u8 = 0x00;
    pub const REDIRECTION: u8 = 0x01;
    pub const INITIATOR_ERROR: u8 = 0x02;
    pub const TARGET_ERROR: u8 = 0x03;

    // Common status detail codes
    pub const SUCCESS_ACCEPT: u16 = 0x0000;
    pub const TARGET_MOVED_TEMPORARILY: u16 = 0x0101;
    pub const TARGET_MOVED_PERMANENTLY: u16 = 0x0102;
    pub const INITIATOR_ERROR_GENERIC: u16 = 0x0200;
    pub const AUTH_FAILURE: u16 = 0x0201;
    pub const AUTHORIZATION_FAILURE: u16 = 0x0202;
    pub const TARGET_NOT_FOUND: u16 = 0x0203;
    pub const TARGET_REMOVED: u16 = 0x0204;
    pub const UNSUPPORTED_VERSION: u16 = 0x0205;
    pub const TOO_MANY_CONNECTIONS: u16 = 0x0206;
    pub const MISSING_PARAMETER: u16 = 0x0207;
    pub const CANT_INCLUDE_IN_SESSION: u16 = 0x0208;
    pub const SESSION_TYPE_NOT_SUPPORTED: u16 = 0x0209;
    pub const SESSION_DOES_NOT_EXIST: u16 = 0x020A;
    pub const INVALID_DURING_LOGIN: u16 = 0x020B;
    pub const TARGET_ERROR_GENERIC: u16 = 0x0300;
    pub const SERVICE_UNAVAILABLE: u16 = 0x0301;
    pub const OUT_OF_RESOURCES: u16 = 0x0302;
}

/// SCSI response status codes
pub mod scsi_status {
    pub const GOOD: u8 = 0x00;
    pub const CHECK_CONDITION: u8 = 0x02;
    pub const CONDITION_MET: u8 = 0x04;
    pub const BUSY: u8 = 0x08;
    pub const RESERVATION_CONFLICT: u8 = 0x18;
    pub const TASK_SET_FULL: u8 = 0x28;
    pub const ACA_ACTIVE: u8 = 0x30;
    pub const TASK_ABORTED: u8 = 0x40;
}

/// Basic Header Segment (BHS) - 48 bytes
///
/// ```text
/// Byte/     0       |       1       |       2       |       3       |
///     /              |               |               |               |
///    |0 1 2 3 4 5 6 7|0 1 2 3 4 5 6 7|0 1 2 3 4 5 6 7|0 1 2 3 4 5 6 7|
///    +---------------+---------------+---------------+---------------+
///   0|.|I| Opcode    |F|  Opcode-specific fields                     |
///    +---------------+---------------+---------------+---------------+
///   4|TotalAHSLength | DataSegmentLength                             |
///    +---------------+---------------+---------------+---------------+
///   8| LUN or Opcode-specific fields                                 |
///    +                                                               +
///  12|                                                               |
///    +---------------+---------------+---------------+---------------+
///  16| Initiator Task Tag                                            |
///    +---------------+---------------+---------------+---------------+
///  20| Opcode-specific fields (28 bytes)                             |
///    +                                                               +
///  ...
///  44|                                                               |
///    +---------------+---------------+---------------+---------------+
/// ```
#[derive(Debug, Clone)]
pub struct IscsiPdu {
    /// Opcode identifies the PDU type (lower 6 bits of byte 0)
    pub opcode: u8,
    /// Immediate flag (bit 6 of byte 0)
    pub immediate: bool,
    /// Opcode-specific flags (byte 1)
    pub flags: u8,
    /// Bytes 2-3: For Login Request this is version info (version_max, version_min)
    /// For other PDUs this is reserved/opcode-specific
    pub version_or_reserved: u16,
    /// Total AHS (Additional Header Segment) length (4-byte units)
    pub ahs_length: u8,
    /// Data segment length (bytes)
    pub data_length: u32,
    /// Logical Unit Number (bytes 8-15)
    pub lun: u64,
    /// Initiator Task Tag (bytes 16-19)
    pub itt: u32,
    /// Opcode-specific fields (bytes 20-47, 28 bytes)
    pub specific: [u8; 28],
    /// Data segment (variable length)
    pub data: Vec<u8>,
}

impl Default for IscsiPdu {
    fn default() -> Self {
        Self::new()
    }
}

impl IscsiPdu {
    /// Create a new empty PDU
    pub fn new() -> Self {
        IscsiPdu {
            opcode: 0,
            immediate: false,
            flags: 0,
            version_or_reserved: 0,
            ahs_length: 0,
            data_length: 0,
            lun: 0,
            itt: 0,
            specific: [0u8; 28],
            data: Vec::new(),
        }
    }

    /// Parse a PDU from bytes
    ///
    /// The input buffer must contain at least the 48-byte BHS.
    /// If the PDU has data, the buffer must also contain the data segment.
    pub fn from_bytes(buf: &[u8]) -> ScsiResult<Self> {
        if buf.len() < BHS_SIZE {
            return Err(IscsiError::InvalidPdu(format!(
                "PDU too short: {} bytes, need at least {}",
                buf.len(),
                BHS_SIZE
            )));
        }

        let mut cursor = Cursor::new(buf);

        // Byte 0: Immediate flag (bit 6) and Opcode (bits 0-5)
        let byte0 = cursor.read_u8().map_err(IscsiError::Io)?;
        let immediate = (byte0 & 0x40) != 0;
        let opcode = byte0 & 0x3F;

        // Byte 1: Flags (opcode-specific)
        let flags = cursor.read_u8().map_err(IscsiError::Io)?;

        // Bytes 2-3: For Login Request, this is version info; for others, reserved
        let version_or_reserved = cursor.read_u16::<BigEndian>().map_err(IscsiError::Io)?;

        // Byte 4: Total AHS Length (4-byte units)
        let ahs_length = cursor.read_u8().map_err(IscsiError::Io)?;

        // Bytes 5-7: Data Segment Length (3 bytes, big-endian)
        let ds_len_high = cursor.read_u8().map_err(IscsiError::Io)? as u32;
        let ds_len_low = cursor.read_u16::<BigEndian>().map_err(IscsiError::Io)? as u32;
        let data_length = (ds_len_high << 16) | ds_len_low;

        // Bytes 8-15: LUN
        let lun = cursor.read_u64::<BigEndian>().map_err(IscsiError::Io)?;

        // Bytes 16-19: Initiator Task Tag
        let itt = cursor.read_u32::<BigEndian>().map_err(IscsiError::Io)?;

        // Bytes 20-47: Opcode-specific fields
        let mut specific = [0u8; 28];
        std::io::Read::read_exact(&mut cursor, &mut specific).map_err(IscsiError::Io)?;

        // Calculate total expected length (BHS + AHS + data + padding)
        let ahs_bytes = (ahs_length as usize) * 4;
        let padded_data_len = (data_length as usize).div_ceil(4) * 4; // Pad to 4-byte boundary
        let total_len = BHS_SIZE + ahs_bytes + padded_data_len;

        if buf.len() < total_len {
            return Err(IscsiError::InvalidPdu(format!(
                "PDU incomplete: {} bytes, need {} (BHS={}, AHS={}, data={})",
                buf.len(),
                total_len,
                BHS_SIZE,
                ahs_bytes,
                padded_data_len
            )));
        }

        // Extract data segment (skip AHS for now)
        let data_start = BHS_SIZE + ahs_bytes;
        let data = buf[data_start..data_start + data_length as usize].to_vec();

        Ok(IscsiPdu {
            opcode,
            immediate,
            flags,
            version_or_reserved,
            ahs_length,
            data_length,
            lun,
            itt,
            specific,
            data,
        })
    }

    /// Serialize PDU to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let ahs_bytes = (self.ahs_length as usize) * 4;
        let padded_data_len = self.data.len().div_ceil(4) * 4;
        let total_len = BHS_SIZE + ahs_bytes + padded_data_len;

        let mut buf = Vec::with_capacity(total_len);

        // Byte 0: Immediate flag and Opcode
        let byte0 = (if self.immediate { 0x40 } else { 0 }) | (self.opcode & 0x3F);
        buf.push(byte0);

        // Byte 1: Flags
        buf.push(self.flags);

        // Bytes 2-3: Reserved (opcode-specific)
        // Special case for SCSI Response: bytes 2-3 are Response and Status
        // Special case for SCSI Data-In: byte 3 is Status if S bit is set
        // Special case for Login Request/Response: bytes 2-3 are version info
        if self.opcode == opcode::SCSI_RESPONSE {
            buf.push(self.specific[0]); // Response (byte 2)
            buf.push(self.specific[1]); // Status (byte 3)
        } else if self.opcode == opcode::SCSI_DATA_IN && (self.flags & 0x01) != 0 {
            buf.push(0); // Reserved (byte 2)
            buf.push((self.version_or_reserved & 0xFF) as u8); // Status (byte 3)
        } else if self.opcode == opcode::LOGIN_REQUEST || self.opcode == opcode::LOGIN_RESPONSE {
            // Write version_or_reserved for Login PDUs
            buf.push((self.version_or_reserved >> 8) as u8); // High byte (version-max or active version)
            buf.push((self.version_or_reserved & 0xFF) as u8); // Low byte (version-min or reserved)
        } else {
            buf.push(0);
            buf.push(0);
        }

        // Byte 4: Total AHS Length
        buf.push(self.ahs_length);

        // Bytes 5-7: Data Segment Length (3 bytes, big-endian)
        let data_len = self.data.len() as u32;
        buf.push(((data_len >> 16) & 0xFF) as u8);
        buf.write_u16::<BigEndian>((data_len & 0xFFFF) as u16).unwrap();

        // Bytes 8-15: LUN field
        // According to RFC 3720, LUN is only in:
        // - SCSI Command PDU
        // - SCSI Data-Out PDU
        // - Task Management Function PDU
        // All other PDUs should have reserved/0 in this field
        let write_lun = matches!(self.opcode, opcode::SCSI_COMMAND | opcode::SCSI_DATA_OUT | opcode::TASK_MANAGEMENT_REQUEST);
        if write_lun {
            buf.write_u64::<BigEndian>(self.lun).unwrap();
        } else {
            // For other PDU types (including SCSI Response), this field is reserved and should be 0
            buf.write_u64::<BigEndian>(0).unwrap();
        }

        // Bytes 16-19: Initiator Task Tag
        buf.write_u32::<BigEndian>(self.itt).unwrap();

        // Bytes 20-47: Opcode-specific fields
        buf.extend_from_slice(&self.specific);

        // AHS (if any) - not implemented yet, would go here

        // Data segment
        buf.extend_from_slice(&self.data);

        // Pad to 4-byte boundary
        while buf.len() < total_len {
            buf.push(0);
        }

        buf
    }

    /// Get the opcode name for debugging
    pub fn opcode_name(&self) -> &'static str {
        match self.opcode {
            opcode::NOP_OUT => "NOP-Out",
            opcode::SCSI_COMMAND => "SCSI Command",
            opcode::TASK_MANAGEMENT_REQUEST => "Task Management Request",
            opcode::LOGIN_REQUEST => "Login Request",
            opcode::TEXT_REQUEST => "Text Request",
            opcode::SCSI_DATA_OUT => "SCSI Data-Out",
            opcode::LOGOUT_REQUEST => "Logout Request",
            opcode::SNACK_REQUEST => "SNACK Request",
            opcode::NOP_IN => "NOP-In",
            opcode::SCSI_RESPONSE => "SCSI Response",
            opcode::TASK_MANAGEMENT_RESPONSE => "Task Management Response",
            opcode::LOGIN_RESPONSE => "Login Response",
            opcode::TEXT_RESPONSE => "Text Response",
            opcode::SCSI_DATA_IN => "SCSI Data-In",
            opcode::LOGOUT_RESPONSE => "Logout Response",
            opcode::R2T => "Ready To Transfer",
            opcode::ASYNC_MESSAGE => "Async Message",
            opcode::REJECT => "Reject",
            _ => "Unknown",
        }
    }

    /// Get the total PDU length including headers and padded data
    pub fn total_length(&self) -> usize {
        let ahs_bytes = (self.ahs_length as usize) * 4;
        let padded_data_len = self.data.len().div_ceil(4) * 4;
        BHS_SIZE + ahs_bytes + padded_data_len
    }
}

// ============================================================================
// Login Request/Response PDU helpers
// ============================================================================

/// Login Request PDU parsing helpers
impl IscsiPdu {
    /// Create a Login Request PDU
    pub fn login_request(
        isid: [u8; 6],
        tsih: u16,
        cid: u16,
        cmd_sn: u32,
        exp_stat_sn: u32,
        csg: u8,
        nsg: u8,
        transit: bool,
        data: Vec<u8>,
    ) -> Self {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::LOGIN_REQUEST;
        pdu.immediate = true;

        // Flags: Transit | Continue | CSG | NSG
        pdu.flags = (if transit { flags::TRANSIT } else { 0 })
            | ((csg & 0x03) << 2)
            | (nsg & 0x03);

        // ISID in LUN field (bytes 8-13 of BHS)
        let mut lun_bytes = [0u8; 8];
        lun_bytes[0..6].copy_from_slice(&isid);
        lun_bytes[6..8].copy_from_slice(&tsih.to_be_bytes());
        pdu.lun = u64::from_be_bytes(lun_bytes);

        // Opcode-specific fields
        // Bytes 20-21: CID
        pdu.specific[0..2].copy_from_slice(&cid.to_be_bytes());
        // Bytes 24-27: CmdSN
        pdu.specific[4..8].copy_from_slice(&cmd_sn.to_be_bytes());
        // Bytes 28-31: ExpStatSN
        pdu.specific[8..12].copy_from_slice(&exp_stat_sn.to_be_bytes());

        pdu.data = data;
        pdu.data_length = pdu.data.len() as u32;

        pdu
    }

    /// Parse Login Request fields
    pub fn parse_login_request(&self) -> ScsiResult<LoginRequest> {
        if self.opcode != opcode::LOGIN_REQUEST {
            return Err(IscsiError::InvalidPdu(format!(
                "Expected Login Request opcode 0x03, got 0x{:02x}",
                self.opcode
            )));
        }

        let lun_bytes = self.lun.to_be_bytes();
        let mut isid = [0u8; 6];
        isid.copy_from_slice(&lun_bytes[0..6]);
        let tsih = BigEndian::read_u16(&lun_bytes[6..8]);

        let transit = (self.flags & flags::TRANSIT) != 0;
        let cont = (self.flags & flags::CONTINUE_LOGIN) != 0;
        let csg = (self.flags >> 2) & 0x03;
        let nsg = self.flags & 0x03;

        let cid = BigEndian::read_u16(&self.specific[0..2]);
        let cmd_sn = BigEndian::read_u32(&self.specific[4..8]);
        let exp_stat_sn = BigEndian::read_u32(&self.specific[8..12]);

        // Extract version info from bytes 2-3
        let version_max = (self.version_or_reserved >> 8) as u8;
        let version_min = (self.version_or_reserved & 0xFF) as u8;

        Ok(LoginRequest {
            isid,
            tsih,
            cid,
            cmd_sn,
            exp_stat_sn,
            transit,
            cont,
            csg,
            nsg,
            version_max,
            version_min,
            parameters: parse_text_parameters(&self.data)?,
        })
    }

    /// Create a Login Response PDU
    pub fn login_response(
        isid: [u8; 6],
        tsih: u16,
        stat_sn: u32,
        exp_cmd_sn: u32,
        max_cmd_sn: u32,
        status_class: u8,
        status_detail: u8,
        csg: u8,
        nsg: u8,
        transit: bool,
        itt: u32,
        data: Vec<u8>,
    ) -> Self {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::LOGIN_RESPONSE;

        // Flags: Transit | Continue | CSG | NSG
        pdu.flags = (if transit { flags::TRANSIT } else { 0 })
            | ((csg & 0x03) << 2)
            | (nsg & 0x03);

        // ISID + TSIH in LUN field
        let mut lun_bytes = [0u8; 8];
        lun_bytes[0..6].copy_from_slice(&isid);
        lun_bytes[6..8].copy_from_slice(&tsih.to_be_bytes());
        pdu.lun = u64::from_be_bytes(lun_bytes);

        pdu.itt = itt;

        // Opcode-specific fields
        // Bytes 24-27: StatSN
        pdu.specific[4..8].copy_from_slice(&stat_sn.to_be_bytes());
        // Bytes 28-31: ExpCmdSN
        pdu.specific[8..12].copy_from_slice(&exp_cmd_sn.to_be_bytes());
        // Bytes 32-35: MaxCmdSN
        pdu.specific[12..16].copy_from_slice(&max_cmd_sn.to_be_bytes());
        // Bytes 36-37: Status-Class and Status-Detail
        pdu.specific[16] = status_class;
        pdu.specific[17] = status_detail;

        pdu.data = data;
        pdu.data_length = pdu.data.len() as u32;

        pdu
    }
}

/// Parsed Login Request
#[derive(Debug, Clone)]
pub struct LoginRequest {
    pub isid: [u8; 6],
    pub tsih: u16,
    pub cid: u16,
    pub cmd_sn: u32,
    pub exp_stat_sn: u32,
    pub transit: bool,
    pub cont: bool,
    pub csg: u8,
    pub nsg: u8,
    pub version_max: u8, // RFC 3720: highest version initiator supports
    pub version_min: u8, // RFC 3720: lowest version initiator supports
    pub parameters: Vec<(String, String)>,
}

// ============================================================================
// SCSI Command/Response PDU helpers
// ============================================================================

impl IscsiPdu {
    /// Parse SCSI Command PDU
    pub fn parse_scsi_command(&self) -> ScsiResult<ScsiCommandPdu> {
        if self.opcode != opcode::SCSI_COMMAND {
            return Err(IscsiError::InvalidPdu(format!(
                "Expected SCSI Command opcode 0x01, got 0x{:02x}",
                self.opcode
            )));
        }

        let read = (self.flags & flags::READ) != 0;
        let write = (self.flags & flags::WRITE) != 0;
        let final_flag = (self.flags & flags::FINAL) != 0;

        let expected_data_length = BigEndian::read_u32(&self.specific[0..4]);

        // CDB is in specific[12..28] (16 bytes)
        let mut cdb = [0u8; 16];
        cdb.copy_from_slice(&self.specific[12..28]);

        Ok(ScsiCommandPdu {
            lun: self.lun,
            itt: self.itt,
            expected_data_length,
            cdb,
            read,
            write,
            final_flag,
        })
    }

    /// Create a SCSI Response PDU
    pub fn scsi_response(
        itt: u32,
        stat_sn: u32,
        exp_cmd_sn: u32,
        max_cmd_sn: u32,
        status: u8,
        response: u8,
        residual_count: u32,
        sense_data: Option<&[u8]>,
    ) -> Self {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::SCSI_RESPONSE;
        pdu.flags = flags::FINAL; // Always final for response
        pdu.itt = itt;

        // NOTE: Response and Status go in BHS bytes 2-3, which are NOT part of the 'specific' array!
        // The 'specific' array starts at byte 20 of the BHS.
        // We need to store them in reserved[0] and reserved[1] which we'll handle in to_bytes()
        // For now, store them in specific for backwards compat with serialization
        // Actually, we need to refactor to add response/status fields to IscsiPdu

        // WORKAROUND: Store response/status in a way that to_bytes() will handle
        // We'll need to modify the serialization to put these in the right place
        pdu.specific[0] = response; // Will be moved to BHS byte 2 during serialization
        pdu.specific[1] = status;   // Will be moved to BHS byte 3 during serialization

        // StatSN at bytes 24-27 (specific[4..8])
        pdu.specific[4..8].copy_from_slice(&stat_sn.to_be_bytes());
        // ExpCmdSN at bytes 28-31 (specific[8..12])
        pdu.specific[8..12].copy_from_slice(&exp_cmd_sn.to_be_bytes());
        // MaxCmdSN at bytes 32-35 (specific[12..16])
        pdu.specific[12..16].copy_from_slice(&max_cmd_sn.to_be_bytes());

        // Add sense data if provided
        // RFC 3720 Section 10.4.7: Data segment = [SenseLength (2 bytes)] [Sense Data]
        if let Some(sense) = sense_data {
            let sense_len = sense.len() as u16;
            let mut data_segment = Vec::with_capacity(2 + sense.len());
            data_segment.extend_from_slice(&sense_len.to_be_bytes());
            data_segment.extend_from_slice(sense);
            pdu.data_length = data_segment.len() as u32;
            pdu.data = data_segment;
        }

        // Residual count at bytes 40-43 (specific[20..24])
        pdu.specific[20..24].copy_from_slice(&residual_count.to_be_bytes());

        pdu
    }

    /// Create a SCSI Data-In PDU (data from target to initiator)
    pub fn scsi_data_in(
        itt: u32,
        ttt: u32,
        stat_sn: u32,
        exp_cmd_sn: u32,
        max_cmd_sn: u32,
        data_sn: u32,
        buffer_offset: u32,
        data: Vec<u8>,
        final_flag: bool,
        status: Option<u8>,
    ) -> Self {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::SCSI_DATA_IN;

        // Flags
        let mut flags_byte = 0u8;
        if final_flag {
            flags_byte |= flags::FINAL;
        }
        if status.is_some() {
            flags_byte |= flags::DATA_IN_STATUS;
        }
        pdu.flags = flags_byte;

        pdu.itt = itt;

        // Target Transfer Tag
        pdu.specific[0..4].copy_from_slice(&ttt.to_be_bytes());
        // StatSN (only valid if S bit set)
        pdu.specific[4..8].copy_from_slice(&stat_sn.to_be_bytes());
        // ExpCmdSN
        pdu.specific[8..12].copy_from_slice(&exp_cmd_sn.to_be_bytes());
        // MaxCmdSN
        pdu.specific[12..16].copy_from_slice(&max_cmd_sn.to_be_bytes());
        // DataSN
        pdu.specific[16..20].copy_from_slice(&data_sn.to_be_bytes());
        // Buffer Offset
        pdu.specific[20..24].copy_from_slice(&buffer_offset.to_be_bytes());
        // Residual count (for underflow/overflow)
        // pdu.specific[24..28] - residual count if needed

        if let Some(s) = status {
            // Status occupies BHS byte 3 and must not share storage with the
            // residual count in bytes 44-47.
            pdu.version_or_reserved = s as u16;
        }

        pdu.data = data;
        pdu.data_length = pdu.data.len() as u32;

        pdu
    }

    /// Apply RFC 7143 residual semantics to a final SCSI Data-In PDU.
    pub fn set_data_in_residual(
        &mut self,
        expected_data_length: u32,
        transferred_data_length: u32,
    ) {
        self.flags &= !(flags::DATA_IN_OVERFLOW | flags::DATA_IN_UNDERFLOW);
        let residual_count = match transferred_data_length.cmp(&expected_data_length) {
            std::cmp::Ordering::Greater => {
                self.flags |= flags::DATA_IN_OVERFLOW;
                transferred_data_length - expected_data_length
            }
            std::cmp::Ordering::Less => {
                self.flags |= flags::DATA_IN_UNDERFLOW;
                expected_data_length - transferred_data_length
            }
            std::cmp::Ordering::Equal => 0,
        };
        self.specific[24..28].copy_from_slice(&residual_count.to_be_bytes());
    }

    /// Parse SCSI Data-Out PDU (data from initiator to target)
    pub fn parse_scsi_data_out(&self) -> ScsiResult<ScsiDataOutPdu> {
        if self.opcode != opcode::SCSI_DATA_OUT {
            return Err(IscsiError::InvalidPdu(format!(
                "Expected SCSI Data-Out opcode 0x05, got 0x{:02x}",
                self.opcode
            )));
        }

        let final_flag = (self.flags & flags::FINAL) != 0;
        let ttt = BigEndian::read_u32(&self.specific[0..4]);
        let exp_stat_sn = BigEndian::read_u32(&self.specific[4..8]);
        let data_sn = BigEndian::read_u32(&self.specific[16..20]);
        let buffer_offset = BigEndian::read_u32(&self.specific[20..24]);

        Ok(ScsiDataOutPdu {
            lun: self.lun,
            itt: self.itt,
            ttt,
            exp_stat_sn,
            data_sn,
            buffer_offset,
            data: self.data.clone(),
            final_flag,
        })
    }
}

/// Parsed SCSI Command
#[derive(Debug, Clone)]
pub struct ScsiCommandPdu {
    pub lun: u64,
    pub itt: u32,
    pub expected_data_length: u32,
    pub cdb: [u8; 16],
    pub read: bool,
    pub write: bool,
    pub final_flag: bool,
}

/// Parsed SCSI Data-Out
#[derive(Debug, Clone)]
pub struct ScsiDataOutPdu {
    pub lun: u64,
    pub itt: u32,
    pub ttt: u32,
    pub exp_stat_sn: u32,
    pub data_sn: u32,
    pub buffer_offset: u32,
    pub data: Vec<u8>,
    pub final_flag: bool,
}

// ============================================================================
// R2T (Ready To Transfer) PDU helpers
// ============================================================================

impl IscsiPdu {
    /// Create an R2T (Ready To Transfer) PDU
    ///
    /// R2T is sent by the target to request data from the initiator for a write command
    /// when the initiator has more data to send beyond the FirstBurstLength.
    ///
    /// RFC 3720 Section 10.8
    pub fn r2t(
        lun: u64,
        itt: u32,
        ttt: u32,
        stat_sn: u32,
        exp_cmd_sn: u32,
        max_cmd_sn: u32,
        r2t_sn: u32,
        buffer_offset: u32,
        desired_data_transfer_length: u32,
    ) -> Self {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::R2T;
        pdu.flags = flags::FINAL; // R2T always has F bit set
        pdu.lun = lun;
        pdu.itt = itt;

        // Target Transfer Tag (bytes 20-23)
        pdu.specific[0..4].copy_from_slice(&ttt.to_be_bytes());
        // StatSN (bytes 24-27)
        pdu.specific[4..8].copy_from_slice(&stat_sn.to_be_bytes());
        // ExpCmdSN (bytes 28-31)
        pdu.specific[8..12].copy_from_slice(&exp_cmd_sn.to_be_bytes());
        // MaxCmdSN (bytes 32-35)
        pdu.specific[12..16].copy_from_slice(&max_cmd_sn.to_be_bytes());
        // R2TSN (bytes 36-39)
        pdu.specific[16..20].copy_from_slice(&r2t_sn.to_be_bytes());
        // Buffer Offset (bytes 40-43)
        pdu.specific[20..24].copy_from_slice(&buffer_offset.to_be_bytes());
        // Desired Data Transfer Length (bytes 44-47)
        pdu.specific[24..28].copy_from_slice(&desired_data_transfer_length.to_be_bytes());

        pdu
    }
}

// ============================================================================
// NOP-Out/NOP-In PDU helpers
// ============================================================================

impl IscsiPdu {
    /// Create a NOP-In PDU (target → initiator, usually response to NOP-Out)
    pub fn nop_in(
        itt: u32,
        ttt: u32,
        stat_sn: u32,
        exp_cmd_sn: u32,
        max_cmd_sn: u32,
        lun: u64,
    ) -> Self {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::NOP_IN;
        pdu.flags = flags::FINAL;
        pdu.lun = lun;
        pdu.itt = itt;

        // Target Transfer Tag
        pdu.specific[0..4].copy_from_slice(&ttt.to_be_bytes());
        // StatSN
        pdu.specific[4..8].copy_from_slice(&stat_sn.to_be_bytes());
        // ExpCmdSN
        pdu.specific[8..12].copy_from_slice(&exp_cmd_sn.to_be_bytes());
        // MaxCmdSN
        pdu.specific[12..16].copy_from_slice(&max_cmd_sn.to_be_bytes());

        pdu
    }

    /// Parse NOP-Out PDU
    pub fn parse_nop_out(&self) -> ScsiResult<NopOutPdu> {
        if self.opcode != opcode::NOP_OUT {
            return Err(IscsiError::InvalidPdu(format!(
                "Expected NOP-Out opcode 0x00, got 0x{:02x}",
                self.opcode
            )));
        }

        let ttt = BigEndian::read_u32(&self.specific[0..4]);
        let cmd_sn = BigEndian::read_u32(&self.specific[4..8]);
        let exp_stat_sn = BigEndian::read_u32(&self.specific[8..12]);

        Ok(NopOutPdu {
            lun: self.lun,
            itt: self.itt,
            ttt,
            cmd_sn,
            exp_stat_sn,
            data: self.data.clone(),
        })
    }
}

/// Parsed NOP-Out
#[derive(Debug, Clone)]
pub struct NopOutPdu {
    pub lun: u64,
    pub itt: u32,
    pub ttt: u32,
    pub cmd_sn: u32,
    pub exp_stat_sn: u32,
    pub data: Vec<u8>,
}

// ============================================================================
// Logout Request/Response PDU helpers
// ============================================================================

/// Logout reason codes
pub mod logout_reason {
    pub const CLOSE_SESSION: u8 = 0;
    pub const CLOSE_CONNECTION: u8 = 1;
    pub const REMOVE_CONNECTION_FOR_RECOVERY: u8 = 2;
}

/// Logout response codes
pub mod logout_response {
    pub const SUCCESS: u8 = 0;
    pub const CID_NOT_FOUND: u8 = 1;
    pub const CONNECTION_RECOVERY_NOT_SUPPORTED: u8 = 2;
    pub const CLEANUP_FAILED: u8 = 3;
}

impl IscsiPdu {
    /// Parse Logout Request
    pub fn parse_logout_request(&self) -> ScsiResult<LogoutRequest> {
        if self.opcode != opcode::LOGOUT_REQUEST {
            return Err(IscsiError::InvalidPdu(format!(
                "Expected Logout Request opcode 0x06, got 0x{:02x}",
                self.opcode
            )));
        }

        let reason = self.flags & 0x7F;
        let cid = BigEndian::read_u16(&self.specific[0..2]);
        let cmd_sn = BigEndian::read_u32(&self.specific[4..8]);
        let exp_stat_sn = BigEndian::read_u32(&self.specific[8..12]);

        Ok(LogoutRequest {
            itt: self.itt,
            reason,
            cid,
            cmd_sn,
            exp_stat_sn,
        })
    }

    /// Create a Logout Response PDU
    pub fn logout_response(
        itt: u32,
        stat_sn: u32,
        exp_cmd_sn: u32,
        max_cmd_sn: u32,
        response: u8,
        time2wait: u16,
        time2retain: u16,
    ) -> Self {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::LOGOUT_RESPONSE;
        pdu.flags = flags::FINAL;
        pdu.itt = itt;

        // Response code
        pdu.specific[0] = response;
        // StatSN
        pdu.specific[4..8].copy_from_slice(&stat_sn.to_be_bytes());
        // ExpCmdSN
        pdu.specific[8..12].copy_from_slice(&exp_cmd_sn.to_be_bytes());
        // MaxCmdSN
        pdu.specific[12..16].copy_from_slice(&max_cmd_sn.to_be_bytes());
        // Time2Wait
        pdu.specific[20..22].copy_from_slice(&time2wait.to_be_bytes());
        // Time2Retain
        pdu.specific[22..24].copy_from_slice(&time2retain.to_be_bytes());

        pdu
    }
}

/// Parsed Logout Request
#[derive(Debug, Clone)]
pub struct LogoutRequest {
    pub itt: u32,
    pub reason: u8,
    pub cid: u16,
    pub cmd_sn: u32,
    pub exp_stat_sn: u32,
}

// ============================================================================
// Text Request/Response PDU helpers
// ============================================================================

impl IscsiPdu {
    /// Parse Text Request
    pub fn parse_text_request(&self) -> ScsiResult<TextRequest> {
        if self.opcode != opcode::TEXT_REQUEST {
            return Err(IscsiError::InvalidPdu(format!(
                "Expected Text Request opcode 0x04, got 0x{:02x}",
                self.opcode
            )));
        }

        let final_flag = (self.flags & flags::FINAL) != 0;
        let cont = (self.flags & flags::CONTINUE) != 0;
        let ttt = BigEndian::read_u32(&self.specific[0..4]);
        let cmd_sn = BigEndian::read_u32(&self.specific[4..8]);
        let exp_stat_sn = BigEndian::read_u32(&self.specific[8..12]);

        Ok(TextRequest {
            itt: self.itt,
            ttt,
            cmd_sn,
            exp_stat_sn,
            final_flag,
            cont,
            parameters: parse_text_parameters(&self.data)?,
        })
    }

    /// Create a Text Response PDU
    pub fn text_response(
        itt: u32,
        ttt: u32,
        stat_sn: u32,
        exp_cmd_sn: u32,
        max_cmd_sn: u32,
        final_flag: bool,
        data: Vec<u8>,
    ) -> Self {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::TEXT_RESPONSE;
        pdu.flags = if final_flag { flags::FINAL } else { 0 };
        pdu.itt = itt;

        // Target Transfer Tag
        pdu.specific[0..4].copy_from_slice(&ttt.to_be_bytes());
        // StatSN
        pdu.specific[4..8].copy_from_slice(&stat_sn.to_be_bytes());
        // ExpCmdSN
        pdu.specific[8..12].copy_from_slice(&exp_cmd_sn.to_be_bytes());
        // MaxCmdSN
        pdu.specific[12..16].copy_from_slice(&max_cmd_sn.to_be_bytes());

        pdu.data = data;
        pdu.data_length = pdu.data.len() as u32;

        pdu
    }
}

/// Parsed Text Request
#[derive(Debug, Clone)]
pub struct TextRequest {
    pub itt: u32,
    pub ttt: u32,
    pub cmd_sn: u32,
    pub exp_stat_sn: u32,
    pub final_flag: bool,
    pub cont: bool,
    pub parameters: Vec<(String, String)>,
}

// ============================================================================
// Utility functions
// ============================================================================

/// Parse iSCSI text parameters (null-terminated key=value pairs)
pub fn parse_text_parameters(data: &[u8]) -> ScsiResult<Vec<(String, String)>> {
    let mut params = Vec::new();

    if data.is_empty() {
        return Ok(params);
    }

    // Split on null bytes
    for chunk in data.split(|&b| b == 0) {
        if chunk.is_empty() {
            continue;
        }

        let s = String::from_utf8_lossy(chunk);
        if let Some(eq_pos) = s.find('=') {
            let key = s[..eq_pos].to_string();
            let value = s[eq_pos + 1..].to_string();
            params.push((key, value));
        }
    }

    Ok(params)
}

/// Serialize text parameters to null-terminated format
pub fn serialize_text_parameters(params: &[(String, String)]) -> Vec<u8> {
    let mut data = Vec::new();
    for (key, value) in params {
        data.extend_from_slice(key.as_bytes());
        data.push(b'=');
        data.extend_from_slice(value.as_bytes());
        data.push(0);
    }
    data
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdu_new() {
        let pdu = IscsiPdu::new();
        assert_eq!(pdu.opcode, 0);
        assert!(!pdu.immediate);
        assert_eq!(pdu.flags, 0);
        assert_eq!(pdu.data_length, 0);
        assert!(pdu.data.is_empty());
    }

    #[test]
    fn test_pdu_roundtrip_simple() {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::NOP_OUT;
        pdu.flags = flags::FINAL;
        pdu.itt = 0x12345678;
        pdu.lun = 0x0001020304050607;

        let bytes = pdu.to_bytes();
        assert_eq!(bytes.len(), BHS_SIZE); // No data, so just BHS

        let parsed = IscsiPdu::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.opcode, opcode::NOP_OUT);
        assert_eq!(parsed.flags, flags::FINAL);
        assert_eq!(parsed.itt, 0x12345678);
        // NOP_OUT doesn't preserve LUN field - it's reserved per RFC 3720
        assert_eq!(parsed.lun, 0);
    }

    #[test]
    fn test_pdu_roundtrip_with_data() {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::LOGIN_REQUEST;
        pdu.data = b"InitiatorName=iqn.test\0".to_vec();
        pdu.data_length = pdu.data.len() as u32;

        let bytes = pdu.to_bytes();
        // BHS + data + padding to 4-byte boundary
        assert!(bytes.len() >= BHS_SIZE + pdu.data.len());
        assert_eq!(bytes.len() % 4, 0);

        let parsed = IscsiPdu::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.opcode, opcode::LOGIN_REQUEST);
        assert_eq!(parsed.data, pdu.data);
    }

    #[test]
    fn test_pdu_too_short() {
        let bytes = vec![0u8; 20]; // Too short for BHS
        let result = IscsiPdu::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_text_parameters() {
        let data = b"Key1=Value1\0Key2=Value2\0";
        let params = parse_text_parameters(data).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], ("Key1".to_string(), "Value1".to_string()));
        assert_eq!(params[1], ("Key2".to_string(), "Value2".to_string()));
    }

    #[test]
    fn test_serialize_text_parameters() {
        let params = vec![
            ("Key1".to_string(), "Value1".to_string()),
            ("Key2".to_string(), "Value2".to_string()),
        ];
        let data = serialize_text_parameters(&params);
        assert_eq!(data, b"Key1=Value1\0Key2=Value2\0");
    }

    #[test]
    fn test_login_response_creation() {
        let isid = [0x00, 0x02, 0x3D, 0x00, 0x00, 0x00];
        let pdu = IscsiPdu::login_response(
            isid,
            1,     // tsih
            1,     // stat_sn
            1,     // exp_cmd_sn
            1,     // max_cmd_sn
            0,     // status class (success)
            0,     // status detail
            0,     // csg
            3,     // nsg (full feature)
            true,  // transit
            0x1234, // itt
            vec![], // no data
        );

        assert_eq!(pdu.opcode, opcode::LOGIN_RESPONSE);
        assert_eq!(pdu.flags & flags::TRANSIT, flags::TRANSIT);
        assert_eq!(pdu.itt, 0x1234);
    }

    #[test]
    fn test_scsi_response_creation() {
        let pdu = IscsiPdu::scsi_response(
            0x1234,  // itt
            1,       // stat_sn
            1,       // exp_cmd_sn
            1,       // max_cmd_sn
            scsi_status::GOOD, // status
            0,       // response (completed)
            0,       // residual count
            None,    // no sense data
        );

        assert_eq!(pdu.opcode, opcode::SCSI_RESPONSE);
        assert_eq!(pdu.flags, flags::FINAL);
        assert_eq!(pdu.itt, 0x1234);
        assert_eq!(pdu.specific[1], scsi_status::GOOD);
    }

    #[test]
    fn test_scsi_data_in_creation() {
        let data = vec![0xAB; 512];
        let pdu = IscsiPdu::scsi_data_in(
            0x1234,  // itt
            0xFFFF_FFFF, // ttt
            1,       // stat_sn
            1,       // exp_cmd_sn
            1,       // max_cmd_sn
            0,       // data_sn
            0,       // buffer_offset
            data.clone(),
            true,    // final
            Some(scsi_status::GOOD),
        );

        assert_eq!(pdu.opcode, opcode::SCSI_DATA_IN);
        assert_eq!(pdu.flags & flags::FINAL, flags::FINAL);
        assert_eq!(pdu.data, data);
    }

    #[test]
    fn test_nop_in_creation() {
        let pdu = IscsiPdu::nop_in(
            0x1234,      // itt
            0xFFFF_FFFF, // ttt
            1,           // stat_sn
            1,           // exp_cmd_sn
            1,           // max_cmd_sn
            0,           // lun
        );

        assert_eq!(pdu.opcode, opcode::NOP_IN);
        assert_eq!(pdu.flags, flags::FINAL);
        assert_eq!(pdu.itt, 0x1234);
    }

    #[test]
    fn test_logout_response_creation() {
        let pdu = IscsiPdu::logout_response(
            0x1234,  // itt
            1,       // stat_sn
            1,       // exp_cmd_sn
            1,       // max_cmd_sn
            logout_response::SUCCESS,
            2,       // time2wait
            20,      // time2retain
        );

        assert_eq!(pdu.opcode, opcode::LOGOUT_RESPONSE);
        assert_eq!(pdu.specific[0], logout_response::SUCCESS);
    }

    #[test]
    fn test_opcode_names() {
        let mut pdu = IscsiPdu::new();

        pdu.opcode = opcode::LOGIN_REQUEST;
        assert_eq!(pdu.opcode_name(), "Login Request");

        pdu.opcode = opcode::SCSI_COMMAND;
        assert_eq!(pdu.opcode_name(), "SCSI Command");

        pdu.opcode = opcode::SCSI_RESPONSE;
        assert_eq!(pdu.opcode_name(), "SCSI Response");

        pdu.opcode = 0xFF;
        assert_eq!(pdu.opcode_name(), "Unknown");
    }

    #[test]
    fn test_immediate_flag() {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::LOGIN_REQUEST;
        pdu.immediate = true;

        let bytes = pdu.to_bytes();
        assert_eq!(bytes[0] & 0x40, 0x40); // Immediate bit set

        let parsed = IscsiPdu::from_bytes(&bytes).unwrap();
        assert!(parsed.immediate);
    }

    #[test]
    fn test_data_padding() {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::TEXT_REQUEST;
        pdu.data = vec![1, 2, 3]; // 3 bytes, should pad to 4

        let bytes = pdu.to_bytes();
        assert_eq!(bytes.len() % 4, 0);
        assert_eq!(bytes.len(), BHS_SIZE + 4); // BHS + 4 bytes (padded data)
    }
}
