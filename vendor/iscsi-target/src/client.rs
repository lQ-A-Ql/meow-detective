//! iSCSI Client library for testing and initiator functionality
//!
//! This module provides a low-level iSCSI client implementation focused on
//! enabling test scenarios with arbitrary PDU transmission and reception.
//!
//! # Overview
//!
//! The client module provides:
//! - Raw TCP socket connection to iSCSI targets
//! - PDU transmission and reception
//! - Session state management
//! - Login/logout phases
//! - SCSI command execution
//! - Arbitrary PDU transmission for testing edge cases
//!
//! # Example: Basic Connection and Login
//!
//! ```no_run
//! use iscsi_target::client::IscsiClient;
//!
//! # fn test() -> Result<(), Box<dyn std::error::Error>> {
//! let mut client = IscsiClient::connect("127.0.0.1:3260")?;
//! client.login(
//!     "iqn.2025-12.local:initiator",
//!     "iqn.2025-12.local:storage.disk1",
//! )?;
//! // ... send SCSI commands ...
//! client.logout()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Example: Raw PDU Transmission (for testing)
//!
//! ```no_run
//! use iscsi_target::client::IscsiClient;
//! use iscsi_target::pdu::IscsiPdu;
//!
//! # fn test() -> Result<(), Box<dyn std::error::Error>> {
//! let mut client = IscsiClient::connect("127.0.0.1:3260")?;
//!
//! // Send custom/malformed PDU for testing
//! let mut pdu = IscsiPdu::new();
//! pdu.opcode = 0x99;  // Invalid opcode
//! client.send_raw_pdu(&pdu)?;
//!
//! // Receive response
//! let response = client.recv_pdu()?;
//! # Ok(())
//! # }
//! ```

use byteorder::BigEndian;
use byteorder::ByteOrder;
use crate::error::{IscsiError, ScsiResult, decode_login_status};
use crate::pdu::{self, IscsiPdu, opcode, flags, BHS_SIZE};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

// Negotiated parameters (must match what login sends)
const MAX_RECV_DATA_SEGMENT_LENGTH: u32 = 8192;
const FIRST_BURST_LENGTH: u32 = 65536;


/// iSCSI Client for connecting to targets and sending/receiving PDUs
///
/// The client maintains a TCP connection to the target and handles
/// PDU serialization/deserialization.
pub struct IscsiClient {
    stream: TcpStream,
    cmd_sn: u32,
    exp_stat_sn: u32,
    max_cmd_sn: u32,
    initialized: bool,
    header_digest: bool,
    data_digest: bool,
}

impl IscsiClient {
    /// Connect to an iSCSI target at the given address
    ///
    /// # Arguments
    ///
    /// * `addr` - Address and port in format "host:port" (e.g., "127.0.0.1:3260")
    ///
    /// # Errors
    ///
    /// Returns an error if the TCP connection fails
    pub fn connect(addr: &str) -> ScsiResult<Self> {
        let stream = TcpStream::connect(addr)
            .map_err(IscsiError::Io)?;

        // Set blocking mode and timeouts
        stream.set_nonblocking(false)
            .map_err(IscsiError::Io)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(IscsiError::Io)?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))
            .map_err(IscsiError::Io)?;

        Ok(IscsiClient {
            stream,
            cmd_sn: 0,
            exp_stat_sn: 0,
            max_cmd_sn: u32::MAX,
            initialized: false,
            header_digest: false,
            data_digest: false,
        })
    }

    /// Perform iSCSI login (security negotiation + operational negotiation + full feature phase)
    ///
    /// # Arguments
    ///
    /// * `initiator_name` - IQN of the initiator (e.g., "iqn.2025-12.local:initiator")
    /// * `target_name` - IQN of the target (e.g., "iqn.2025-12.local:storage.disk1")
    ///
    /// # Errors
    ///
    /// Returns an error if login fails at any phase
    pub fn login(&mut self, initiator_name: &str, target_name: &str) -> ScsiResult<()> {
        // Phase 1: Security Negotiation
        self.login_phase(
            initiator_name,
            target_name,
            flags::CSG_SECURITY_NEG,
            flags::NSG_LOGIN_OP_NEG,
            false,
        )?;

        // Phase 2: Operational Negotiation (transitions to Full Feature Phase)
        self.login_phase(
            initiator_name,
            target_name,
            flags::CSG_LOGIN_OP_NEG,
            flags::NSG_FULL_FEATURE,
            true,
        )?;

        // After Phase 2 completes with transit=true, we're in Full Feature Phase
        // No Phase 3 needed - you can't send login PDUs with CSG=3 (FullFeature)

        self.initialized = true;
        Ok(())
    }

    /// Perform a single login phase
    fn login_phase(
        &mut self,
        initiator_name: &str,
        target_name: &str,
        csg: u8,
        nsg: u8,
        transit: bool,
    ) -> ScsiResult<()> {
        // Build login request parameters
        let mut params = String::new();
        params.push_str(&format!("InitiatorName={}\0", initiator_name));
        params.push_str(&format!("TargetName={}\0", target_name));

        if csg == flags::CSG_SECURITY_NEG {
            params.push_str("AuthMethod=None\0");
        }

        if csg == flags::CSG_LOGIN_OP_NEG {
            params.push_str("HeaderDigest=CRC32C,None\0");
            params.push_str("DataDigest=CRC32C,None\0");
            params.push_str("MaxRecvDataSegmentLength=8192\0");
            params.push_str("MaxBurstLength=262144\0");
            params.push_str("FirstBurstLength=65536\0");
            params.push_str("DefaultTime2Wait=2\0");
            params.push_str("DefaultTime2Retain=20\0");
            params.push_str("MaxOutstandingR2T=1\0");
            params.push_str("ImmediateData=Yes\0");
            params.push_str("InitialR2TIOV=0\0");
            params.push_str("DataPDUInOrder=Yes\0");
            params.push_str("DataSequenceInOrder=Yes\0");
            params.push_str("ErrorRecoveryLevel=0\0");
            params.push_str("SessionType=Normal\0");
        }

        // Pad to 4-byte boundary
        while params.len() % 4 != 0 {
            params.push('\0');
        }

        // Create login request PDU
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::LOGIN_REQUEST;
        pdu.immediate = true;
        pdu.flags = if transit { flags::TRANSIT } else { 0 };
        pdu.flags |= (csg & 0x03) << 2; // Current stage
        pdu.flags |= nsg & 0x03;        // Next stage
        pdu.itt = self.cmd_sn;
        pdu.specific[0] = 0; // Version max
        pdu.specific[1] = 0; // Version active
        // CmdSN at specific[4..8]
        BigEndian::write_u32(&mut pdu.specific[4..8], self.cmd_sn);
        // ExpStatSN at specific[8..12]
        BigEndian::write_u32(&mut pdu.specific[8..12], self.exp_stat_sn);
        pdu.data = params.into_bytes();

        self.send_pdu(&pdu)?;

        let response = self.recv_pdu()?;

        if response.opcode != opcode::LOGIN_RESPONSE {
            return Err(IscsiError::InvalidPdu(format!(
                "Expected LOGIN_RESPONSE (0x23), got opcode 0x{:02x}",
                response.opcode
            )));
        }

        // Status-Class and Status-Detail at specific[16..18]
        let status_class = response.specific[16];
        let status_detail = response.specific[17];

        if status_class != pdu::login_status::SUCCESS {
            let decoded_message = decode_login_status(status_class, status_detail);
            return Err(IscsiError::Protocol(format!(
                "Login failed (class=0x{:02x}, detail=0x{:02x})\n\n{}",
                status_class, status_detail, decoded_message
            )));
        }

        // Sync sequence numbers from response:
        // specific[4..8]  = StatSN
        // specific[8..12] = ExpCmdSN (what target expects next)
        // specific[12..16] = MaxCmdSN
        self.exp_stat_sn = BigEndian::read_u32(&response.specific[4..8]).wrapping_add(1);
        self.cmd_sn = BigEndian::read_u32(&response.specific[8..12]);
        self.max_cmd_sn = BigEndian::read_u32(&response.specific[12..16]);

        // Parse negotiated digests from response text parameters
        if transit && !response.data.is_empty() {
            if let Ok(params) = pdu::parse_text_parameters(&response.data) {
                for (key, value) in &params {
                    match key.as_str() {
                        "HeaderDigest" if value == "CRC32C" => self.header_digest = true,
                        "DataDigest" if value == "CRC32C" => self.data_digest = true,
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    /// Discover available targets at the connected portal
    ///
    /// Performs SendTargets discovery to get a list of available iSCSI targets.
    ///
    /// # Arguments
    ///
    /// * `initiator_name` - IQN of the initiator
    ///
    /// # Returns
    ///
    /// A vector of tuples containing (target_iqn, target_address)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use iscsi_target::client::IscsiClient;
    ///
    /// # fn test() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut client = IscsiClient::connect("127.0.0.1:3260")?;
    /// let targets = client.discover("iqn.2025-12.local:initiator")?;
    /// for (iqn, addr) in targets {
    ///     println!("Target: {} at {}", iqn, addr);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn discover(&mut self, initiator_name: &str) -> ScsiResult<Vec<(String, String)>> {
        // Perform discovery login (SessionType=Discovery)
        self.discovery_login(initiator_name)?;

        // Send SendTargets Text Request
        let mut params = String::new();
        params.push_str("SendTargets=All\0");
        while params.len() % 4 != 0 {
            params.push('\0');
        }

        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::TEXT_REQUEST;
        pdu.flags = flags::FINAL;
        pdu.itt = self.cmd_sn;
        // TTT = 0xFFFFFFFF for new request
        pdu.specific[0..4].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        // CmdSN
        pdu.specific[4..8].copy_from_slice(&self.cmd_sn.to_be_bytes());
        // ExpStatSN
        pdu.specific[8..12].copy_from_slice(&self.exp_stat_sn.to_be_bytes());
        pdu.data = params.into_bytes();

        // Send text request
        self.send_pdu(&pdu)?;

        // Receive text response
        let response = self.recv_pdu()?;

        if response.opcode != opcode::TEXT_RESPONSE {
            return Err(IscsiError::InvalidPdu(format!(
                "Expected TEXT_RESPONSE (0x24), got opcode 0x{:02x}",
                response.opcode
            )));
        }

        // Parse response parameters
        let params = pdu::parse_text_parameters(&response.data)?;

        // Extract target information
        let mut targets = Vec::new();
        let mut current_target: Option<String> = None;

        for (key, value) in params {
            match key.as_str() {
                "TargetName" => {
                    current_target = Some(value);
                }
                "TargetAddress" => {
                    if let Some(iqn) = current_target.take() {
                        // TargetAddress format is "host:port,portal-group-tag"
                        // We just need the host:port part
                        let addr = value.split(',').next().unwrap_or(&value).to_string();
                        targets.push((iqn, addr));
                    }
                }
                _ => {}
            }
        }

        // For discovery sessions, we can just close the connection
        // (logout not needed - connection will be closed when client is dropped)

        Ok(targets)
    }

    /// Perform discovery login (SessionType=Discovery)
    fn discovery_login(&mut self, initiator_name: &str) -> ScsiResult<()> {
        // Phase 1: Security Negotiation
        self.discovery_login_phase(
            initiator_name,
            flags::CSG_SECURITY_NEG,
            flags::NSG_LOGIN_OP_NEG,
            false,
        )?;

        // Phase 2: Operational Negotiation with SessionType=Discovery
        self.discovery_login_phase(
            initiator_name,
            flags::CSG_LOGIN_OP_NEG,
            flags::NSG_FULL_FEATURE,
            true,
        )?;

        self.initialized = true;
        Ok(())
    }

    /// Perform a single discovery login phase
    fn discovery_login_phase(
        &mut self,
        initiator_name: &str,
        csg: u8,
        nsg: u8,
        transit: bool,
    ) -> ScsiResult<()> {
        // Build login request parameters
        let mut params = String::new();
        params.push_str(&format!("InitiatorName={}\0", initiator_name));

        if csg == flags::CSG_SECURITY_NEG {
            params.push_str("AuthMethod=None\0");
            params.push_str("SessionType=Discovery\0");
        }

        if csg == flags::CSG_LOGIN_OP_NEG {
            params.push_str("HeaderDigest=None\0");
            params.push_str("DataDigest=None\0");
            params.push_str("MaxRecvDataSegmentLength=8192\0");
            params.push_str("DefaultTime2Wait=2\0");
            params.push_str("DefaultTime2Retain=20\0");
            params.push_str("ErrorRecoveryLevel=0\0");
        }

        // Pad to 4-byte boundary
        while params.len() % 4 != 0 {
            params.push('\0');
        }

        // Create login request PDU
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::LOGIN_REQUEST;
        pdu.immediate = true;
        pdu.flags = if transit { flags::TRANSIT } else { 0 };
        pdu.flags |= (csg & 0x03) << 2; // Current stage
        pdu.flags |= nsg & 0x03;        // Next stage
        pdu.itt = self.cmd_sn; // Use cmd_sn as itt
        pdu.specific[0] = 0; // Version max
        pdu.specific[1] = 0; // Version active
        pdu.data = params.into_bytes();

        // Send login request
        self.send_pdu(&pdu)?;

        // Receive login response
        let response = self.recv_pdu()?;

        // Verify login response
        if response.opcode != opcode::LOGIN_RESPONSE {
            return Err(IscsiError::InvalidPdu(format!(
                "Expected LOGIN_RESPONSE (0x23), got opcode 0x{:02x}",
                response.opcode
            )));
        }

        // Extract status from response
        let status_class = response.specific[0];
        let status_detail = response.specific[1];

        if status_class != pdu::login_status::SUCCESS {
            let decoded_message = decode_login_status(status_class, status_detail);
            return Err(IscsiError::Protocol(format!(
                "Discovery login failed (class=0x{:02x}, detail=0x{:02x})\n\n{}",
                status_class, status_detail, decoded_message
            )));
        }

        // Update sequence numbers from response
        if response.specific.len() >= 16 {
            self.exp_stat_sn = u32::from_be_bytes([
                response.specific[4],
                response.specific[5],
                response.specific[6],
                response.specific[7],
            ]);
            self.max_cmd_sn = u32::from_be_bytes([
                response.specific[8],
                response.specific[9],
                response.specific[10],
                response.specific[11],
            ]);
        }

        // Increment cmd_sn for next command
        self.cmd_sn = self.cmd_sn.wrapping_add(1);

        Ok(())
    }

    /// Send a PDU to the target, with CRC32C digests if negotiated.
    pub fn send_pdu(&mut self, pdu: &IscsiPdu) -> ScsiResult<()> {
        let bytes = pdu.to_bytes();

        // Write BHS
        self.stream.write_all(&bytes[..BHS_SIZE]).map_err(IscsiError::Io)?;

        // Header digest
        if self.header_digest {
            let crc = crc32c::crc32c(&bytes[..BHS_SIZE]);
            self.stream.write_all(&crc.to_le_bytes()).map_err(IscsiError::Io)?;
        }

        // AHS + data segment
        if bytes.len() > BHS_SIZE {
            self.stream.write_all(&bytes[BHS_SIZE..]).map_err(IscsiError::Io)?;
        }

        // Data digest
        if self.data_digest && bytes.len() > BHS_SIZE {
            let crc = crc32c::crc32c(&bytes[BHS_SIZE..]);
            self.stream.write_all(&crc.to_le_bytes()).map_err(IscsiError::Io)?;
        }

        self.stream.flush().map_err(IscsiError::Io)?;
        Ok(())
    }

    /// Send a raw PDU to the target for testing purposes
    ///
    /// This allows sending arbitrary/malformed PDUs for edge case testing.
    pub fn send_raw_pdu(&mut self, pdu: &IscsiPdu) -> ScsiResult<()> {
        self.send_pdu(pdu)
    }

    /// Receive a PDU from the target, verifying CRC32C digests if negotiated.
    pub fn recv_pdu(&mut self) -> ScsiResult<IscsiPdu> {
        let mut bhs = [0u8; BHS_SIZE];
        self.stream.read_exact(&mut bhs).map_err(IscsiError::Io)?;

        // Verify header digest
        if self.header_digest {
            let mut hd = [0u8; 4];
            self.stream.read_exact(&mut hd).map_err(IscsiError::Io)?;
            let expected = u32::from_le_bytes(hd);
            let actual = crc32c::crc32c(&bhs);
            if expected != actual {
                return Err(IscsiError::Protocol(format!(
                    "Header digest mismatch: expected 0x{:08x}, got 0x{:08x}", expected, actual
                )));
            }
        }

        let data_len = ((bhs[5] as u32) << 16) | ((bhs[6] as u32) << 8) | (bhs[7] as u32);
        let padded_len = ((data_len + 3) / 4) * 4;

        let mut buf = bhs.to_vec();
        if padded_len > 0 {
            let mut data_buf = vec![0u8; padded_len as usize];
            self.stream.read_exact(&mut data_buf).map_err(IscsiError::Io)?;
            buf.extend_from_slice(&data_buf);

            // Verify data digest
            if self.data_digest {
                let mut dd = [0u8; 4];
                self.stream.read_exact(&mut dd).map_err(IscsiError::Io)?;
                let expected = u32::from_le_bytes(dd);
                let actual = crc32c::crc32c(&data_buf);
                if expected != actual {
                    return Err(IscsiError::Protocol(format!(
                        "Data digest mismatch: expected 0x{:08x}, got 0x{:08x}", expected, actual
                    )));
                }
            }
        }

        IscsiPdu::from_bytes(&buf)
    }

    /// Send a SCSI command and receive a single response.
    ///
    /// For multi-PDU writes (R2T flow), use `send_write` instead.
    /// For reads that return data, use `send_read` instead.
    pub fn send_scsi_command(&mut self, cdb: &[u8], data_out: Option<&[u8]>) -> ScsiResult<IscsiPdu> {
        if !self.initialized {
            return Err(IscsiError::Session(
                "Not logged in. Call login() first.".to_string(),
            ));
        }
        if cdb.len() > 16 {
            return Err(IscsiError::InvalidPdu(format!(
                "CDB too long: {} bytes (max 16)", cdb.len()
            )));
        }

        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::SCSI_COMMAND;
        pdu.flags = flags::FINAL;
        if data_out.is_some() {
            pdu.flags |= flags::WRITE;
        }
        pdu.itt = self.cmd_sn;
        pdu.lun = 0;

        // specific[0..4] = ExpectedDataTransferLength
        let edtl = data_out.map_or(0u32, |d| d.len() as u32);
        BigEndian::write_u32(&mut pdu.specific[0..4], edtl);
        // specific[4..8] = CmdSN
        BigEndian::write_u32(&mut pdu.specific[4..8], self.cmd_sn);
        // specific[8..12] = ExpStatSN
        BigEndian::write_u32(&mut pdu.specific[8..12], self.exp_stat_sn);
        // specific[12..28] = CDB (16 bytes, zero-padded)
        pdu.specific[12..12 + cdb.len()].copy_from_slice(cdb);

        if let Some(data) = data_out {
            pdu.data = data.to_vec();
        }

        self.send_pdu(&pdu)?;
        self.cmd_sn = self.cmd_sn.wrapping_add(1);

        self.recv_pdu()
    }

    /// Perform iSCSI logout
    pub fn logout(&mut self) -> ScsiResult<()> {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::LOGOUT_REQUEST;
        pdu.immediate = true;
        pdu.flags = flags::FINAL;
        pdu.itt = self.cmd_sn;

        // CmdSN at specific[4..8], ExpStatSN at specific[8..12]
        BigEndian::write_u32(&mut pdu.specific[4..8], self.cmd_sn);
        BigEndian::write_u32(&mut pdu.specific[8..12], self.exp_stat_sn);

        self.send_pdu(&pdu)?;
        let _response = self.recv_pdu()?;

        self.initialized = false;
        Ok(())
    }

    /// Get the current command sequence number
    pub fn cmd_sn(&self) -> u32 {
        self.cmd_sn
    }

    /// Get the current expected status sequence number
    pub fn exp_stat_sn(&self) -> u32 {
        self.exp_stat_sn
    }

    /// Get the maximum command sequence number from target
    pub fn max_cmd_sn(&self) -> u32 {
        self.max_cmd_sn
    }

    /// Check if client is logged in (in full feature phase)
    pub fn is_logged_in(&self) -> bool {
        self.initialized
    }

    /// Receive a PDU, transparently handling NOP-In keepalives.
    pub fn recv_pdu_skip_nop(&mut self) -> ScsiResult<IscsiPdu> {
        loop {
            let pdu = self.recv_pdu()?;
            if pdu.opcode == opcode::NOP_IN {
                // Respond with NOP-Out
                let mut resp = IscsiPdu::new();
                resp.opcode = opcode::NOP_OUT;
                resp.immediate = true;
                resp.flags = flags::FINAL;
                resp.itt = 0xFFFF_FFFF; // unsolicited
                // TTT from the NOP-In
                resp.specific[0..4].copy_from_slice(&pdu.specific[0..4]);
                BigEndian::write_u32(&mut resp.specific[4..8], self.cmd_sn);
                BigEndian::write_u32(&mut resp.specific[8..12], self.exp_stat_sn);
                self.send_pdu(&resp)?;
                continue;
            }
            return Ok(pdu);
        }
    }

    /// Send a SCSI Data-Out PDU (does not increment CmdSN).
    fn send_data_out(
        &mut self,
        itt: u32,
        ttt: u32,
        data_sn: u32,
        buffer_offset: u32,
        data: &[u8],
        final_flag: bool,
    ) -> ScsiResult<()> {
        let mut pdu = IscsiPdu::new();
        pdu.opcode = opcode::SCSI_DATA_OUT;
        pdu.flags = if final_flag { flags::FINAL } else { 0 };
        pdu.lun = 0;
        pdu.itt = itt;
        // specific[0..4] = TTT
        BigEndian::write_u32(&mut pdu.specific[0..4], ttt);
        // specific[4..8] = ExpStatSN
        BigEndian::write_u32(&mut pdu.specific[4..8], self.exp_stat_sn);
        // specific[16..20] = DataSN
        BigEndian::write_u32(&mut pdu.specific[16..20], data_sn);
        // specific[20..24] = BufferOffset
        BigEndian::write_u32(&mut pdu.specific[20..24], buffer_offset);
        pdu.data = data.to_vec();
        self.send_pdu(&pdu)
    }

    /// Write data to a block device via iSCSI, handling multi-PDU R2T flow.
    ///
    /// Builds a WRITE(10) CDB and handles the full Data-Out / R2T sequence.
    pub fn send_write(&mut self, lba: u32, data: &[u8]) -> ScsiResult<IscsiPdu> {
        if !self.initialized {
            return Err(IscsiError::Session("Not logged in".to_string()));
        }
        let block_size: u32 = 512;
        let num_blocks = (data.len() as u32) / block_size;
        let total_len = data.len() as u32;

        // Build WRITE(10) CDB
        let mut cdb = [0u8; 10];
        cdb[0] = 0x2A; // WRITE(10)
        BigEndian::write_u32(&mut cdb[2..6], lba);
        BigEndian::write_u16(&mut cdb[7..9], num_blocks as u16);

        // Immediate data: up to MaxRecvDataSegmentLength
        let immediate_len = (total_len as usize).min(MAX_RECV_DATA_SEGMENT_LENGTH as usize);
        let itt = self.cmd_sn;

        // Build SCSI Command PDU
        let mut cmd = IscsiPdu::new();
        cmd.opcode = opcode::SCSI_COMMAND;
        cmd.lun = 0;
        cmd.itt = itt;

        // Flags: WRITE always set; FINAL only if all data fits in this one PDU
        cmd.flags = flags::WRITE;
        if immediate_len == data.len() {
            cmd.flags |= flags::FINAL;
        }

        // ExpectedDataTransferLength
        BigEndian::write_u32(&mut cmd.specific[0..4], total_len);
        // CmdSN
        BigEndian::write_u32(&mut cmd.specific[4..8], self.cmd_sn);
        // ExpStatSN
        BigEndian::write_u32(&mut cmd.specific[8..12], self.exp_stat_sn);
        // CDB (10 bytes into 16-byte field)
        cmd.specific[12..22].copy_from_slice(&cdb);

        cmd.data = data[..immediate_len].to_vec();

        self.send_pdu(&cmd)?;
        self.cmd_sn = self.cmd_sn.wrapping_add(1);

        // If everything fit in one PDU, wait for response
        if immediate_len == data.len() {
            return self.recv_pdu_skip_nop();
        }

        // Send unsolicited Data-Out burst (up to FirstBurstLength)
        let mut offset = immediate_len as u32;
        let unsolicited_end = total_len.min(FIRST_BURST_LENGTH);
        let mut data_sn: u32 = 0;

        while offset < unsolicited_end {
            let chunk_end = (offset + MAX_RECV_DATA_SEGMENT_LENGTH).min(unsolicited_end);
            let is_last = chunk_end >= unsolicited_end;
            self.send_data_out(
                itt,
                0xFFFF_FFFF, // TTT = 0xFFFFFFFF for unsolicited data
                data_sn,
                offset,
                &data[offset as usize..chunk_end as usize],
                is_last,
            )?;
            offset = chunk_end;
            data_sn += 1;
        }

        // If all data sent in unsolicited burst, wait for response
        if offset >= total_len {
            return self.recv_pdu_skip_nop();
        }

        // R2T loop: target sends R2T, we respond with Data-Out
        loop {
            let pdu = self.recv_pdu_skip_nop()?;

            if pdu.opcode == opcode::R2T {
                let ttt = BigEndian::read_u32(&pdu.specific[0..4]);
                let r2t_offset = BigEndian::read_u32(&pdu.specific[20..24]);
                let r2t_len = BigEndian::read_u32(&pdu.specific[24..28]);
                let r2t_end = r2t_offset + r2t_len;

                let mut off = r2t_offset;
                let mut dsn: u32 = 0;
                while off < r2t_end {
                    let chunk_end = (off + MAX_RECV_DATA_SEGMENT_LENGTH).min(r2t_end);
                    let is_last = chunk_end >= r2t_end;
                    self.send_data_out(
                        itt,
                        ttt,
                        dsn,
                        off,
                        &data[off as usize..chunk_end as usize],
                        is_last,
                    )?;
                    off = chunk_end;
                    dsn += 1;
                }
                continue;
            }

            if pdu.opcode == opcode::SCSI_RESPONSE {
                return Ok(pdu);
            }

            return Err(IscsiError::Protocol(format!(
                "Unexpected opcode 0x{:02x} during write", pdu.opcode
            )));
        }
    }

    /// Read blocks from a block device via iSCSI, collecting Data-In PDUs.
    ///
    /// Returns the SCSI Response PDU and the aggregated read data.
    pub fn send_read(&mut self, lba: u32, num_blocks: u16) -> ScsiResult<(IscsiPdu, Vec<u8>)> {
        if !self.initialized {
            return Err(IscsiError::Session("Not logged in".to_string()));
        }

        // Build READ(10) CDB
        let mut cdb = [0u8; 10];
        cdb[0] = 0x28; // READ(10)
        BigEndian::write_u32(&mut cdb[2..6], lba);
        BigEndian::write_u16(&mut cdb[7..9], num_blocks);

        let expected_len = (num_blocks as u32) * 512;

        let mut cmd = IscsiPdu::new();
        cmd.opcode = opcode::SCSI_COMMAND;
        cmd.flags = flags::FINAL | flags::READ;
        cmd.lun = 0;
        cmd.itt = self.cmd_sn;

        BigEndian::write_u32(&mut cmd.specific[0..4], expected_len);
        BigEndian::write_u32(&mut cmd.specific[4..8], self.cmd_sn);
        BigEndian::write_u32(&mut cmd.specific[8..12], self.exp_stat_sn);
        cmd.specific[12..22].copy_from_slice(&cdb);

        self.send_pdu(&cmd)?;
        self.cmd_sn = self.cmd_sn.wrapping_add(1);

        // Collect Data-In PDUs
        let mut read_data = vec![0u8; expected_len as usize];
        loop {
            let pdu = self.recv_pdu_skip_nop()?;

            if pdu.opcode == opcode::SCSI_DATA_IN {
                let buf_offset = BigEndian::read_u32(&pdu.specific[20..24]) as usize;
                let end = buf_offset + pdu.data.len();
                if end <= read_data.len() {
                    read_data[buf_offset..end].copy_from_slice(&pdu.data);
                }
                // Check if status is piggybacked (S bit = flags bit 0)
                if (pdu.flags & 0x01) != 0 {
                    return Ok((pdu, read_data));
                }
                continue;
            }

            if pdu.opcode == opcode::SCSI_RESPONSE {
                return Ok((pdu, read_data));
            }

            return Err(IscsiError::Protocol(format!(
                "Unexpected opcode 0x{:02x} during read", pdu.opcode
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_client_creation() {
        // This test requires a running target — covered by regression_tests
    }
}
