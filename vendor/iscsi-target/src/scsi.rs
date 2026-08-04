//! SCSI block device trait and command handling
//!
//! This module defines the interface that storage backends must implement
//! and handles SCSI command processing per the SCSI Block Commands (SBC) specification.

use crate::error::{IscsiError, ScsiResult};
use byteorder::{BigEndian, ByteOrder};

/// SCSI block device trait
///
/// Implement this trait to provide storage backend for the iSCSI target.
/// The trait is designed to be simple and focused on block-level operations.
pub trait ScsiBlockDevice: Send + Sync {
    /// Read blocks from the device
    ///
    /// # Arguments
    /// * `lba` - Logical block address to start reading from
    /// * `blocks` - Number of blocks to read
    /// * `block_size` - Size of each block in bytes
    ///
    /// # Returns
    /// Vector containing the requested data (length = blocks * block_size)
    fn read(&self, lba: u64, blocks: u32, block_size: u32) -> ScsiResult<Vec<u8>>;

    /// Write blocks to the device
    ///
    /// # Arguments
    /// * `lba` - Logical block address to start writing to
    /// * `data` - Data to write (length must be multiple of block_size)
    /// * `block_size` - Size of each block in bytes
    fn write(&mut self, lba: u64, data: &[u8], block_size: u32) -> ScsiResult<()>;

    /// Get total capacity in logical blocks
    fn capacity(&self) -> u64;

    /// Get block size in bytes (typically 512 or 4096)
    fn block_size(&self) -> u32;

    /// Whether the device rejects all write operations.
    fn is_read_only(&self) -> bool {
        false
    }

    /// Flush any pending writes to stable storage
    fn flush(&mut self) -> ScsiResult<()> {
        // Default implementation: no-op
        Ok(())
    }

    /// Get vendor identification (8 chars max)
    fn vendor_id(&self) -> &str {
        "ISCSI   "
    }

    /// Get product identification (16 chars max)
    fn product_id(&self) -> &str {
        "Virtual Disk    "
    }

    /// Get product revision (4 chars max)
    fn product_rev(&self) -> &str {
        "1.0 "
    }
}

/// SCSI command opcodes (subset needed for basic block storage)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScsiOpcode {
    TestUnitReady = 0x00,
    RequestSense = 0x03,
    Inquiry = 0x12,
    ModeSense6 = 0x1A,
    StartStopUnit = 0x1B,
    ReadCapacity10 = 0x25,
    Read10 = 0x28,
    Write10 = 0x2A,
    Verify10 = 0x2F,
    SynchronizeCache10 = 0x35,
    ModeSense10 = 0x5A,
    Read16 = 0x88,
    Write16 = 0x8A,
    Verify16 = 0x8F,
    SynchronizeCache16 = 0x91,
    ServiceActionIn16 = 0x9E, // READ CAPACITY 16 uses this
    ReportLuns = 0xA0,
}

impl ScsiOpcode {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x00 => Some(ScsiOpcode::TestUnitReady),
            0x03 => Some(ScsiOpcode::RequestSense),
            0x12 => Some(ScsiOpcode::Inquiry),
            0x1A => Some(ScsiOpcode::ModeSense6),
            0x1B => Some(ScsiOpcode::StartStopUnit),
            0x25 => Some(ScsiOpcode::ReadCapacity10),
            0x28 => Some(ScsiOpcode::Read10),
            0x2A => Some(ScsiOpcode::Write10),
            0x2F => Some(ScsiOpcode::Verify10),
            0x35 => Some(ScsiOpcode::SynchronizeCache10),
            0x5A => Some(ScsiOpcode::ModeSense10),
            0x88 => Some(ScsiOpcode::Read16),
            0x8A => Some(ScsiOpcode::Write16),
            0x8F => Some(ScsiOpcode::Verify16),
            0x91 => Some(ScsiOpcode::SynchronizeCache16),
            0x9E => Some(ScsiOpcode::ServiceActionIn16),
            0xA0 => Some(ScsiOpcode::ReportLuns),
            _ => None,
        }
    }
}

// Keep the old enum name for backwards compatibility
pub type ScsiCommand = ScsiOpcode;

/// SCSI status codes
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

/// SCSI sense key codes
pub mod sense_key {
    pub const NO_SENSE: u8 = 0x00;
    pub const RECOVERED_ERROR: u8 = 0x01;
    pub const NOT_READY: u8 = 0x02;
    pub const MEDIUM_ERROR: u8 = 0x03;
    pub const HARDWARE_ERROR: u8 = 0x04;
    pub const ILLEGAL_REQUEST: u8 = 0x05;
    pub const UNIT_ATTENTION: u8 = 0x06;
    pub const DATA_PROTECT: u8 = 0x07;
    pub const BLANK_CHECK: u8 = 0x08;
    pub const ABORTED_COMMAND: u8 = 0x0B;
    pub const VOLUME_OVERFLOW: u8 = 0x0D;
    pub const MISCOMPARE: u8 = 0x0E;
}

/// Additional Sense Code (ASC) values
pub mod asc {
    pub const NO_ADDITIONAL_SENSE: u8 = 0x00;
    pub const INVALID_COMMAND_OPERATION_CODE: u8 = 0x20;
    pub const LBA_OUT_OF_RANGE: u8 = 0x21;
    pub const INVALID_FIELD_IN_CDB: u8 = 0x24;
    pub const LOGICAL_UNIT_NOT_SUPPORTED: u8 = 0x25;
    pub const WRITE_PROTECTED: u8 = 0x27;
    pub const POWER_ON_RESET: u8 = 0x29;
    pub const MEDIUM_NOT_PRESENT: u8 = 0x3A;
    pub const INTERNAL_TARGET_FAILURE: u8 = 0x44;
}

/// SCSI sense data (fixed format)
#[derive(Debug, Clone)]
pub struct SenseData {
    pub sense_key: u8,
    pub asc: u8,        // Additional Sense Code
    pub ascq: u8,       // Additional Sense Code Qualifier
    pub information: u32,
}

impl SenseData {
    pub fn new(sense_key: u8, asc: u8, ascq: u8) -> Self {
        SenseData {
            sense_key,
            asc,
            ascq,
            information: 0,
        }
    }

    pub fn with_info(mut self, info: u32) -> Self {
        self.information = info;
        self
    }

    /// Serialize to fixed format sense data (18 bytes)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = vec![0u8; 18];

        // Response code: 0x70 = current error, fixed format
        data[0] = 0x70;

        // Sense key
        data[2] = self.sense_key & 0x0F;

        // Information (4 bytes, big-endian)
        BigEndian::write_u32(&mut data[3..7], self.information);

        // Additional sense length
        data[7] = 10; // Remaining bytes after this field

        // ASC and ASCQ
        data[12] = self.asc;
        data[13] = self.ascq;

        data
    }

    /// Create sense data for invalid/unsupported command opcode
    pub fn invalid_command() -> Self {
        SenseData::new(sense_key::ILLEGAL_REQUEST, asc::INVALID_COMMAND_OPERATION_CODE, 0)
    }

    /// Create sense data for LBA out of range
    pub fn lba_out_of_range(lba: u32) -> Self {
        SenseData::new(sense_key::ILLEGAL_REQUEST, asc::LBA_OUT_OF_RANGE, 0)
            .with_info(lba)
    }

    /// Create sense data for medium error
    pub fn medium_error() -> Self {
        SenseData::new(sense_key::MEDIUM_ERROR, 0x11, 0x00) // Unrecovered read error
    }

    /// Create sense data for write protected
    pub fn write_protected() -> Self {
        SenseData::new(sense_key::DATA_PROTECT, asc::WRITE_PROTECTED, 0)
    }
}

/// Result of SCSI command execution
#[derive(Debug, Clone)]
pub struct ScsiResponse {
    /// SCSI status code
    pub status: u8,
    /// Response data (for read commands)
    pub data: Vec<u8>,
    /// Sense data (for CHECK CONDITION status)
    pub sense: Option<SenseData>,
}

impl ScsiResponse {
    /// Create a GOOD status response with data
    pub fn good(data: Vec<u8>) -> Self {
        ScsiResponse {
            status: scsi_status::GOOD,
            data,
            sense: None,
        }
    }

    /// Create a GOOD status response without data
    pub fn good_no_data() -> Self {
        ScsiResponse {
            status: scsi_status::GOOD,
            data: Vec::new(),
            sense: None,
        }
    }

    /// Create a CHECK CONDITION response with sense data
    pub fn check_condition(sense: SenseData) -> Self {
        ScsiResponse {
            status: scsi_status::CHECK_CONDITION,
            data: Vec::new(),
            sense: Some(sense),
        }
    }
}

/// SCSI Command Handler
pub struct ScsiHandler;

impl ScsiHandler {
    /// Handle a SCSI command and return response
    pub fn handle_command(
        cdb: &[u8],
        device: &dyn ScsiBlockDevice,
        write_data: Option<&[u8]>,
    ) -> ScsiResult<ScsiResponse> {
        Self::handle_command_with_target(cdb, device, write_data, None)
    }

    pub fn handle_command_with_target(
        cdb: &[u8],
        device: &dyn ScsiBlockDevice,
        write_data: Option<&[u8]>,
        target_name: Option<&str>,
    ) -> ScsiResult<ScsiResponse> {
        if cdb.is_empty() {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        let opcode = cdb[0];

        // Note: LUN validation is done at the target level since the LUN is in the PDU header,
        // not in the CDB. The handler receives already-validated LUN.

        match ScsiOpcode::from_u8(opcode) {
            Some(ScsiOpcode::TestUnitReady) => Self::handle_test_unit_ready(),
            Some(ScsiOpcode::Inquiry) => Self::handle_inquiry(cdb, device, target_name),
            Some(ScsiOpcode::ReadCapacity10) => Self::handle_read_capacity_10(device),
            Some(ScsiOpcode::ServiceActionIn16) => Self::handle_service_action_in_16(cdb, device),
            Some(ScsiOpcode::Read10) => Self::handle_read_10(cdb, device),
            Some(ScsiOpcode::Read16) => Self::handle_read_16(cdb, device),
            Some(ScsiOpcode::Write10) => Self::handle_write_10(cdb, device, write_data),
            Some(ScsiOpcode::Write16) => Self::handle_write_16(cdb, device, write_data),
            Some(ScsiOpcode::ModeSense6) => Self::handle_mode_sense_6(cdb, device),
            Some(ScsiOpcode::ModeSense10) => Self::handle_mode_sense_10(cdb, device),
            Some(ScsiOpcode::RequestSense) => Self::handle_request_sense(cdb),
            Some(ScsiOpcode::SynchronizeCache10) | Some(ScsiOpcode::SynchronizeCache16) => {
                Self::handle_synchronize_cache(device)
            }
            Some(ScsiOpcode::ReportLuns) => Self::handle_report_luns(cdb),
            Some(ScsiOpcode::StartStopUnit) => Self::handle_start_stop_unit(cdb),
            Some(ScsiOpcode::Verify10) | Some(ScsiOpcode::Verify16) => {
                // VERIFY without BYTCHK just checks the medium - always succeed
                Ok(ScsiResponse::good_no_data())
            }
            None => {
                let sense = SenseData::invalid_command();
                Ok(ScsiResponse::check_condition(sense))
            }
        }
    }

    /// Handle TEST UNIT READY (0x00)
    fn handle_test_unit_ready() -> ScsiResult<ScsiResponse> {
        // Device is always ready
        Ok(ScsiResponse::good_no_data())
    }

    /// Handle INQUIRY (0x12)
    fn handle_inquiry(cdb: &[u8], device: &dyn ScsiBlockDevice, target_name: Option<&str>) -> ScsiResult<ScsiResponse> {
        if cdb.len() < 6 {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        let evpd = cdb[1] & 0x01;
        let page_code = cdb[2];
        let alloc_len = BigEndian::read_u16(&cdb[3..5]) as usize;

        if evpd != 0 {
            // VPD page request
            return Self::handle_inquiry_vpd(page_code, alloc_len, device, target_name);
        }

        // Standard INQUIRY response (36 bytes minimum)
        let mut data = vec![0u8; 96];

        // Peripheral device type: 0x00 = Direct access block device (disk)
        data[0] = 0x00;

        // RMB (Removable media bit) = 0 (not removable)
        data[1] = 0x00;

        // Version: 0x05 = SPC-3
        data[2] = 0x05;

        // Response data format: 0x02 = SPC-3
        // HiSup (hierarchical support) = 1
        data[3] = 0x12;

        // Additional length
        data[4] = 91; // Total length - 4

        // Flags
        data[5] = 0x00; // No special features
        data[6] = 0x00;
        data[7] = 0x02; // CmdQue = 1 (command queuing supported)

        // Vendor identification (8 bytes, space-padded)
        let vendor = device.vendor_id();
        let vendor_bytes = vendor.as_bytes();
        for (i, &b) in vendor_bytes.iter().take(8).enumerate() {
            data[8 + i] = b;
        }
        for i in vendor_bytes.len()..8 {
            data[8 + i] = b' ';
        }

        // Product identification (16 bytes, space-padded)
        let product = device.product_id();
        let product_bytes = product.as_bytes();
        for (i, &b) in product_bytes.iter().take(16).enumerate() {
            data[16 + i] = b;
        }
        for i in product_bytes.len()..16 {
            data[16 + i] = b' ';
        }

        // Product revision (4 bytes, space-padded)
        let rev = device.product_rev();
        let rev_bytes = rev.as_bytes();
        for (i, &b) in rev_bytes.iter().take(4).enumerate() {
            data[32 + i] = b;
        }
        for i in rev_bytes.len()..4 {
            data[32 + i] = b' ';
        }

        // Truncate to allocation length
        data.truncate(alloc_len.min(data.len()));

        Ok(ScsiResponse::good(data))
    }

    /// Handle INQUIRY VPD pages
    fn handle_inquiry_vpd(page_code: u8, alloc_len: usize, _device: &dyn ScsiBlockDevice, target_name: Option<&str>) -> ScsiResult<ScsiResponse> {
        match page_code {
            0x00 => {
                // Supported VPD pages
                let mut data = vec![0x00, 0x00, 0x00, 4]; // Device type, page code, reserved, page length
                data.extend_from_slice(&[0x00, 0x80, 0x83, 0xB0]); // Supported pages
                data.truncate(alloc_len.min(data.len()));
                Ok(ScsiResponse::good(data))
            }
            0x80 => {
                // Unit Serial Number
                let mut data = vec![0x00, 0x80, 0x00, 16]; // Device type, page code, reserved, page length
                data.extend_from_slice(b"ISCSI00000000001"); // 16-char serial
                data.truncate(alloc_len.min(data.len()));
                Ok(ScsiResponse::good(data))
            }
            0x83 => {
                // Device Identification
                let mut data = vec![0x00, 0x83, 0x00, 0x00]; // Header

                // NAA descriptor (binary, NAA type)
                let naa_desc = [
                    0x01, 0x03, 0x00, 0x08, // Code set=binary, type=NAA, length=8
                    0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // NAA-6 identifier
                ];
                data.extend_from_slice(&naa_desc);

                // iSCSI target name descriptor (required for proper device identification)
                if let Some(name) = target_name {
                    let name_bytes = name.as_bytes();
                    // Pad to 4-byte boundary
                    let padded_len = (name_bytes.len() + 3) & !3;
                    data.push(0x02); // Code set = ASCII
                    data.push(0x05); // Association = target port, Designator type = SCSI name string
                    data.push(0x00); // Reserved
                    data.push(padded_len as u8); // Designator length
                    data.extend_from_slice(name_bytes);
                    // Pad with zeros
                    for _ in name_bytes.len()..padded_len {
                        data.push(0);
                    }
                }

                // Update page length (2-byte big-endian at offset 2-3)
                let page_len = data.len() - 4;
                data[2] = (page_len >> 8) as u8;
                data[3] = page_len as u8;

                data.truncate(alloc_len.min(data.len()));
                Ok(ScsiResponse::good(data))
            }
            0xB0 => {
                // Block Limits
                let mut data = vec![0u8; 64];
                data[0] = 0x00; // Device type
                data[1] = 0xB0; // Page code
                BigEndian::write_u16(&mut data[2..4], 60); // Page length

                // Maximum transfer length (in blocks)
                let max_xfer = 65535u32; // Max blocks per transfer
                BigEndian::write_u32(&mut data[8..12], max_xfer);

                // Optimal transfer length
                BigEndian::write_u32(&mut data[12..16], 128); // 128 blocks optimal

                data.truncate(alloc_len.min(data.len()));
                Ok(ScsiResponse::good(data))
            }
            _ => {
                Ok(ScsiResponse::check_condition(SenseData::invalid_command()))
            }
        }
    }

    /// Handle READ CAPACITY (10) - 0x25
    fn handle_read_capacity_10(device: &dyn ScsiBlockDevice) -> ScsiResult<ScsiResponse> {
        let capacity = device.capacity();
        let block_size = device.block_size();

        // Response is 8 bytes: last LBA (4 bytes) + block size (4 bytes)
        let mut data = vec![0u8; 8];

        // Last logical block address (or 0xFFFFFFFF if > 2TB)
        let last_lba = if capacity > 0 { capacity - 1 } else { 0 };
        let last_lba_32 = if last_lba > 0xFFFF_FFFE {
            0xFFFF_FFFF_u32 // Signal to use READ CAPACITY 16
        } else {
            last_lba as u32
        };

        BigEndian::write_u32(&mut data[0..4], last_lba_32);
        BigEndian::write_u32(&mut data[4..8], block_size);

        Ok(ScsiResponse::good(data))
    }

    /// Handle SERVICE ACTION IN (16) - includes READ CAPACITY 16
    fn handle_service_action_in_16(cdb: &[u8], device: &dyn ScsiBlockDevice) -> ScsiResult<ScsiResponse> {
        if cdb.len() < 16 {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        let service_action = cdb[1] & 0x1F;

        if service_action != 0x10 {
            // 0x10 = READ CAPACITY 16
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        let alloc_len = BigEndian::read_u32(&cdb[10..14]) as usize;

        let capacity = device.capacity();
        let block_size = device.block_size();

        // Response is 32 bytes for READ CAPACITY 16
        let mut data = vec![0u8; 32];

        // Last logical block address (8 bytes)
        let last_lba = if capacity > 0 { capacity - 1 } else { 0 };
        BigEndian::write_u64(&mut data[0..8], last_lba);

        // Block size (4 bytes)
        BigEndian::write_u32(&mut data[8..12], block_size);

        // Truncate to allocation length
        data.truncate(alloc_len.min(data.len()));

        Ok(ScsiResponse::good(data))
    }

    /// Handle READ (10) - 0x28
    fn handle_read_10(cdb: &[u8], device: &dyn ScsiBlockDevice) -> ScsiResult<ScsiResponse> {
        if cdb.len() < 10 {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        let lba = BigEndian::read_u32(&cdb[2..6]) as u64;
        let transfer_length = BigEndian::read_u16(&cdb[7..9]) as u32;

        if transfer_length == 0 {
            return Ok(ScsiResponse::good_no_data());
        }

        // Validate LBA range
        let capacity = device.capacity();
        if lba + transfer_length as u64 > capacity {
            return Ok(ScsiResponse::check_condition(SenseData::lba_out_of_range(lba as u32)));
        }

        // Read data
        match device.read(lba, transfer_length, device.block_size()) {
            Ok(data) => Ok(ScsiResponse::good(data)),
            Err(_) => Ok(ScsiResponse::check_condition(SenseData::medium_error())),
        }
    }

    /// Handle READ (16) - 0x88
    fn handle_read_16(cdb: &[u8], device: &dyn ScsiBlockDevice) -> ScsiResult<ScsiResponse> {
        if cdb.len() < 16 {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        let lba = BigEndian::read_u64(&cdb[2..10]);
        let transfer_length = BigEndian::read_u32(&cdb[10..14]);

        if transfer_length == 0 {
            return Ok(ScsiResponse::good_no_data());
        }

        // Validate LBA range
        let capacity = device.capacity();
        if lba + transfer_length as u64 > capacity {
            return Ok(ScsiResponse::check_condition(
                SenseData::lba_out_of_range((lba & 0xFFFF_FFFF) as u32)
            ));
        }

        // Read data
        match device.read(lba, transfer_length, device.block_size()) {
            Ok(data) => Ok(ScsiResponse::good(data)),
            Err(_) => Ok(ScsiResponse::check_condition(SenseData::medium_error())),
        }
    }

    /// Handle WRITE (10) - 0x2A
    fn handle_write_10(
        cdb: &[u8],
        device: &dyn ScsiBlockDevice,
        write_data: Option<&[u8]>,
    ) -> ScsiResult<ScsiResponse> {
        if cdb.len() < 10 {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        if device.is_read_only() {
            return Ok(ScsiResponse::check_condition(SenseData::write_protected()));
        }

        let lba = BigEndian::read_u32(&cdb[2..6]) as u64;
        let transfer_length = BigEndian::read_u16(&cdb[7..9]) as u32;

        if transfer_length == 0 {
            return Ok(ScsiResponse::good_no_data());
        }

        // Validate LBA range
        let capacity = device.capacity();
        if lba + transfer_length as u64 > capacity {
            return Ok(ScsiResponse::check_condition(SenseData::lba_out_of_range(lba as u32)));
        }

        // Check write data
        let data = match write_data {
            Some(d) => d,
            None => {
                return Err(IscsiError::Scsi("Write data required but not provided".into()));
            }
        };

        let expected_len = transfer_length as usize * device.block_size() as usize;
        if data.len() < expected_len {
            return Err(IscsiError::Scsi(format!(
                "Write data too short: got {}, need {}",
                data.len(),
                expected_len
            )));
        }

        // This is a read-only trait reference, so we can't actually write
        // In a real implementation, we'd need &mut dyn ScsiBlockDevice
        // For now, we just validate and return success
        // The actual write happens in the target server which has mutable access

        Ok(ScsiResponse::good_no_data())
    }

    /// Handle WRITE (16) - 0x8A
    fn handle_write_16(
        cdb: &[u8],
        device: &dyn ScsiBlockDevice,
        write_data: Option<&[u8]>,
    ) -> ScsiResult<ScsiResponse> {
        if cdb.len() < 16 {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }


        if device.is_read_only() {
            return Ok(ScsiResponse::check_condition(SenseData::write_protected()));
        }

        let lba = BigEndian::read_u64(&cdb[2..10]);
        let transfer_length = BigEndian::read_u32(&cdb[10..14]);

        if transfer_length == 0 {
            return Ok(ScsiResponse::good_no_data());
        }

        // Validate LBA range
        let capacity = device.capacity();
        if lba + transfer_length as u64 > capacity {
            return Ok(ScsiResponse::check_condition(
                SenseData::lba_out_of_range((lba & 0xFFFF_FFFF) as u32)
            ));
        }

        // Check write data
        let data = match write_data {
            Some(d) => d,
            None => {
                return Err(IscsiError::Scsi("Write data required but not provided".into()));
            }
        };

        let expected_len = transfer_length as usize * device.block_size() as usize;
        if data.len() < expected_len {
            return Err(IscsiError::Scsi(format!(
                "Write data too short: got {}, need {}",
                data.len(),
                expected_len
            )));
        }

        Ok(ScsiResponse::good_no_data())
    }

    /// Handle MODE SENSE (6) - 0x1A
    fn handle_mode_sense_6(
        cdb: &[u8],
        device: &dyn ScsiBlockDevice,
    ) -> ScsiResult<ScsiResponse> {
        if cdb.len() < 6 {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        let page_code = cdb[2] & 0x3F;
        let alloc_len = cdb[4] as usize;

        // Return minimal mode parameter header
        let mut data = vec![0u8; 4];
        data[0] = 3; // Mode data length (excluding this byte)
        data[1] = 0; // Medium type
        data[2] = if device.is_read_only() { 0x80 } else { 0 };
        data[3] = 0; // Block descriptor length

        // Add page data if requested
        if page_code == 0x3F {
            // Return all pages - just return header for now
        }

        data.truncate(alloc_len.min(data.len()));
        Ok(ScsiResponse::good(data))
    }

    /// Handle MODE SENSE (10) - 0x5A
    fn handle_mode_sense_10(
        cdb: &[u8],
        device: &dyn ScsiBlockDevice,
    ) -> ScsiResult<ScsiResponse> {
        if cdb.len() < 10 {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        let _page_code = cdb[2] & 0x3F;
        let alloc_len = BigEndian::read_u16(&cdb[7..9]) as usize;

        // Return minimal mode parameter header (8 bytes for MODE SENSE 10)
        let mut data = vec![0u8; 8];
        BigEndian::write_u16(&mut data[0..2], 6); // Mode data length
        data[2] = 0; // Medium type
        data[3] = if device.is_read_only() { 0x80 } else { 0 };
        data[4] = 0; // Reserved
        data[5] = 0; // Reserved
        BigEndian::write_u16(&mut data[6..8], 0); // Block descriptor length

        data.truncate(alloc_len.min(data.len()));
        Ok(ScsiResponse::good(data))
    }

    /// Handle REQUEST SENSE - 0x03
    fn handle_request_sense(cdb: &[u8]) -> ScsiResult<ScsiResponse> {
        if cdb.len() < 6 {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        let alloc_len = cdb[4] as usize;

        // Return "no sense" - no errors to report
        let sense = SenseData::new(sense_key::NO_SENSE, asc::NO_ADDITIONAL_SENSE, 0);
        let mut data = sense.to_bytes();
        data.truncate(alloc_len.min(data.len()));

        Ok(ScsiResponse::good(data))
    }

    /// Handle SYNCHRONIZE CACHE - 0x35 / 0x91
    fn handle_synchronize_cache(_device: &dyn ScsiBlockDevice) -> ScsiResult<ScsiResponse> {
        // We don't have mutable access here, but we acknowledge the request
        // The actual flush would happen at the target server level
        Ok(ScsiResponse::good_no_data())
    }

    /// Handle REPORT LUNS - 0xA0
    fn handle_report_luns(cdb: &[u8]) -> ScsiResult<ScsiResponse> {
        if cdb.len() < 12 {
            return Ok(ScsiResponse::check_condition(SenseData::invalid_command()));
        }

        let alloc_len = BigEndian::read_u32(&cdb[6..10]) as usize;

        // Report LUN 0 only
        let mut data = vec![0u8; 16];
        BigEndian::write_u32(&mut data[0..4], 8); // LUN list length (1 LUN * 8 bytes)
        // data[4..8] reserved
        // data[8..16] = LUN 0 (all zeros)

        data.truncate(alloc_len.min(data.len()));
        Ok(ScsiResponse::good(data))
    }

    /// Handle START STOP UNIT - 0x1B
    fn handle_start_stop_unit(_cdb: &[u8]) -> ScsiResult<ScsiResponse> {
        // Accept but ignore start/stop commands
        Ok(ScsiResponse::good_no_data())
    }

    /// Parse LBA and transfer length from READ/WRITE 10 CDB
    pub fn parse_rw10_cdb(cdb: &[u8]) -> Option<(u64, u32)> {
        if cdb.len() < 10 {
            return None;
        }
        let lba = BigEndian::read_u32(&cdb[2..6]) as u64;
        let length = BigEndian::read_u16(&cdb[7..9]) as u32;
        Some((lba, length))
    }

    /// Parse LBA and transfer length from READ/WRITE 16 CDB
    pub fn parse_rw16_cdb(cdb: &[u8]) -> Option<(u64, u32)> {
        if cdb.len() < 16 {
            return None;
        }
        let lba = BigEndian::read_u64(&cdb[2..10]);
        let length = BigEndian::read_u32(&cdb[10..14]);
        Some((lba, length))
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock device for testing
    struct MockDevice {
        capacity: u64,
        block_size: u32,
        data: Vec<u8>,
    }

    impl MockDevice {
        fn new(capacity: u64, block_size: u32) -> Self {
            let size = (capacity * block_size as u64) as usize;
            MockDevice {
                capacity,
                block_size,
                data: vec![0u8; size],
            }
        }
    }

    impl ScsiBlockDevice for MockDevice {
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
            self.capacity
        }

        fn block_size(&self) -> u32 {
            self.block_size
        }
    }

    #[test]
    fn test_test_unit_ready() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x00, 0, 0, 0, 0, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
    }

    #[test]
    fn test_inquiry() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x12, 0, 0, 0, 96, 0]; // INQUIRY, alloc_len=96
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
        assert!(!response.data.is_empty());
        assert_eq!(response.data[0], 0x00); // Block device
    }

    #[test]
    fn test_inquiry_vpd_supported_pages() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x12, 0x01, 0x00, 0, 255, 0]; // INQUIRY VPD page 0
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
        assert_eq!(response.data[1], 0x00); // Page code 0
    }

    #[test]
    fn test_read_capacity_10() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x25, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
        assert_eq!(response.data.len(), 8);

        let last_lba = BigEndian::read_u32(&response.data[0..4]);
        let block_size = BigEndian::read_u32(&response.data[4..8]);
        assert_eq!(last_lba, 999); // 1000 blocks, last LBA is 999
        assert_eq!(block_size, 512);
    }

    #[test]
    fn test_read_capacity_16() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x9E, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);

        let last_lba = BigEndian::read_u64(&response.data[0..8]);
        let block_size = BigEndian::read_u32(&response.data[8..12]);
        assert_eq!(last_lba, 999);
        assert_eq!(block_size, 512);
    }

    #[test]
    fn test_read_10() {
        let device = MockDevice::new(1000, 512);
        // READ(10): LBA=0, transfer_length=1
        let cdb = [0x28, 0, 0, 0, 0, 0, 0, 0, 1, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
        assert_eq!(response.data.len(), 512);
    }

    #[test]
    fn test_read_10_out_of_range() {
        let device = MockDevice::new(100, 512);
        // READ(10): LBA=200 (out of range)
        let cdb = [0x28, 0, 0, 0, 0, 200, 0, 0, 1, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::CHECK_CONDITION);
        assert!(response.sense.is_some());
    }

    #[test]
    fn test_mode_sense_6() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x1A, 0, 0x3F, 0, 255, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
    }

    #[test]
    fn test_mode_sense_10() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x5A, 0, 0x3F, 0, 0, 0, 0, 0, 255, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
    }

    #[test]
    fn test_report_luns() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0xA0, 0, 0, 0, 0, 0, 0, 0, 0, 16, 0, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
        assert_eq!(response.data.len(), 16);
    }

    #[test]
    fn test_request_sense() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x03, 0, 0, 0, 18, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
        assert_eq!(response.data.len(), 18);
    }

    #[test]
    fn test_synchronize_cache() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x35, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
    }

    #[test]
    fn test_unsupported_command() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0xFF, 0, 0, 0, 0, 0]; // Invalid opcode
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::CHECK_CONDITION);
        assert!(response.sense.is_some());
        let sense = response.sense.unwrap();
        assert_eq!(sense.sense_key, sense_key::ILLEGAL_REQUEST);
        assert_eq!(sense.asc, asc::INVALID_COMMAND_OPERATION_CODE);
        // Verify sense data serialization
        let sense_bytes = sense.to_bytes();
        assert_eq!(sense_bytes[2], sense_key::ILLEGAL_REQUEST);
        assert_eq!(sense_bytes[12], asc::INVALID_COMMAND_OPERATION_CODE);
    }

    #[test]
    fn test_sense_data_serialization() {
        let sense = SenseData::new(sense_key::ILLEGAL_REQUEST, asc::INVALID_FIELD_IN_CDB, 0);
        let data = sense.to_bytes();
        assert_eq!(data.len(), 18);
        assert_eq!(data[0], 0x70); // Current error, fixed format
        assert_eq!(data[2], sense_key::ILLEGAL_REQUEST);
        assert_eq!(data[12], asc::INVALID_FIELD_IN_CDB);
    }

    #[test]
    fn test_parse_rw10_cdb() {
        let cdb = [0x28, 0, 0, 0, 0, 100, 0, 0, 10, 0]; // LBA=100, length=10
        let (lba, length) = ScsiHandler::parse_rw10_cdb(&cdb).unwrap();
        assert_eq!(lba, 100);
        assert_eq!(length, 10);
    }

    #[test]
    fn test_parse_rw16_cdb() {
        let cdb = [
            0x88, 0,
            0, 0, 0, 0, 0, 0, 0, 100, // LBA=100
            0, 0, 0, 10, // length=10
            0, 0
        ];
        let (lba, length) = ScsiHandler::parse_rw16_cdb(&cdb).unwrap();
        assert_eq!(lba, 100);
        assert_eq!(length, 10);
    }

    #[test]
    fn test_start_stop_unit() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x1B, 0, 0, 0, 0, 0];
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
    }

    #[test]
    fn test_verify() {
        let device = MockDevice::new(1000, 512);
        let cdb = [0x2F, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // VERIFY(10)
        let response = ScsiHandler::handle_command(&cdb, &device, None).unwrap();
        assert_eq!(response.status, scsi_status::GOOD);
    }
}
