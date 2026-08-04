//! iSCSI session management
//!
//! This module handles session state, connection management, and parameter negotiation
//! based on RFC 3720: https://datatracker.ietf.org/doc/html/rfc3720

use crate::auth::{AuthConfig, ChapAuthState};
use crate::error::{IscsiError, ScsiResult};
use crate::pdu::{self, IscsiPdu, LoginRequest, serialize_text_parameters};
use std::collections::HashMap;

/// Session state machine states (RFC 3720 Section 5)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum SessionState {
    /// Initial state, waiting for first login PDU
    #[default]
    Free,
    /// Security negotiation phase (CHAP, etc.)
    SecurityNegotiation,
    /// Login operational parameter negotiation
    LoginOperationalNegotiation,
    /// Full feature phase - ready for SCSI commands
    FullFeaturePhase,
    /// Logout in progress
    Logout,
    /// Session failed/error state
    Failed,
}


/// Session type (RFC 3720 Section 5.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum SessionType {
    /// Normal session for SCSI commands
    #[default]
    Normal,
    /// Discovery session for target discovery (SendTargets)
    Discovery,
}


/// Negotiated session parameters (RFC 3720 Section 12)
#[derive(Debug, Clone)]
pub struct SessionParams {
    // Connection parameters
    /// Maximum data segment length target can receive (default: 8192)
    pub max_recv_data_segment_length: u32,
    /// Maximum data segment length initiator can receive
    pub max_xmit_data_segment_length: u32,

    // Session parameters
    /// Maximum burst length for unsolicited data (default: 262144)
    pub max_burst_length: u32,
    /// First burst length for unsolicited data (default: 65536)
    pub first_burst_length: u32,
    /// Default time to wait before reconnecting (seconds)
    pub default_time2wait: u16,
    /// Default time to retain connection (seconds)
    pub default_time2retain: u16,
    /// Maximum outstanding R2T (Ready to Transfer) PDUs
    pub max_outstanding_r2t: u32,
    /// Data PDU in order (within a sequence)
    pub data_pdu_in_order: bool,
    /// Data sequence in order
    pub data_sequence_in_order: bool,
    /// Error recovery level (0-2)
    pub error_recovery_level: u8,
    /// Immediate data allowed
    pub immediate_data: bool,
    /// Initial R2T required
    pub initial_r2t: bool,

    // Digest settings
    /// Header digest (None, CRC32C)
    pub header_digest: DigestType,
    /// Data digest (None, CRC32C)
    pub data_digest: DigestType,

    // Names
    /// Target name (IQN)
    pub target_name: String,
    /// Initiator name (IQN)
    pub initiator_name: String,
    /// Target alias (optional)
    pub target_alias: String,
    /// Initiator alias (optional)
    pub initiator_alias: String,

    // Validation tracking
    /// Invalid session type received (for error reporting)
    pub(crate) invalid_session_type: Option<String>,
}

/// Digest type for header/data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum DigestType {
    #[default]
    None,
    CRC32C,
}

/// Negotiate digest from initiator's offer (e.g. "CRC32C", "None", "CRC32C,None").
/// RFC 3720 5.2.2: select the first offered value we support, honouring the
/// initiator's preference order; fall back to None.
pub(crate) fn negotiate_digest(offer: &str) -> DigestType {
    for token in offer.split(',') {
        let token = token.trim();
        if token.eq_ignore_ascii_case("CRC32C") {
            return DigestType::CRC32C;
        }
        if token.eq_ignore_ascii_case("None") {
            return DigestType::None;
        }
    }
    DigestType::None
}


impl Default for SessionParams {
    fn default() -> Self {
        SessionParams {
            max_recv_data_segment_length: 8192,
            max_xmit_data_segment_length: 8192,
            max_burst_length: 262144,
            first_burst_length: 65536,
            default_time2wait: 2,
            default_time2retain: 20,
            max_outstanding_r2t: 1,
            data_pdu_in_order: true,
            data_sequence_in_order: true,
            error_recovery_level: 0,
            immediate_data: true,
            initial_r2t: false,  // Allow immediate data without waiting for R2T
            header_digest: DigestType::None,
            data_digest: DigestType::None,
            target_name: String::new(),
            initiator_name: String::new(),
            target_alias: String::new(),
            initiator_alias: String::new(),
            invalid_session_type: None,
        }
    }
}

/// Pending write command information
#[derive(Debug, Clone)]
pub struct PendingWrite {
    /// Logical Block Address from the WRITE command
    pub lba: u64,
    /// Transfer length in blocks
    pub transfer_length: u32,
    /// Block size
    pub block_size: u32,
    /// Total bytes received so far
    pub bytes_received: u32,
    /// Target Transfer Tag (used for R2T correlation)
    pub ttt: u32,
    /// R2T sequence number (incremented for each R2T sent)
    pub r2t_sn: u32,
    /// LUN for this command
    pub lun: u64,
    /// Buffer to accumulate data from multiple DATA-OUT PDUs
    pub buffer: Vec<u8>,
    /// Next offset to request via R2T (respects MaxOutstandingR2T)
    pub next_r2t_offset: u32,
    /// Total expected data length
    pub expected_data_len: u32,
    /// Write already completed — absorb stray Data-Out PDUs silently
    pub completed: bool,
    /// R2T is outstanding — don't send another until this one's data arrives
    pub r2t_pending: bool,
}

/// iSCSI Session
///
/// Represents an active iSCSI session between an initiator and target.
#[derive(Debug, Clone)]
pub struct IscsiSession {
    /// Initiator Session ID (6 bytes)
    pub isid: [u8; 6],
    /// Target Session Identifying Handle (assigned by target)
    pub tsih: u16,
    /// Connection ID
    pub cid: u16,
    /// Session type
    pub session_type: SessionType,
    /// Current session state
    pub state: SessionState,
    /// Negotiated parameters
    pub params: SessionParams,

    // Sequence numbers
    /// Expected command sequence number from initiator
    pub exp_cmd_sn: u32,
    /// Maximum command sequence number initiator can use
    pub max_cmd_sn: u32,
    /// Status sequence number (target → initiator)
    pub stat_sn: u32,

    // Login tracking
    /// Current login stage
    current_stage: u8,
    /// Next login stage
    next_stage: u8,

    // Command tracking
    /// Pending write commands indexed by ITT (Initiator Task Tag)
    pub pending_writes: HashMap<u32, PendingWrite>,
    /// Next Target Transfer Tag (incremented for each new R2T sequence)
    pub next_ttt: u32,
    /// Latest sense data to be returned by REQUEST SENSE
    pub last_sense_data: Option<Vec<u8>>,

    // Authentication
    /// Authentication configuration for this session
    pub auth_config: AuthConfig,
    /// CHAP authentication state for initiator-to-target (if using CHAP)
    pub chap_state: Option<ChapAuthState>,
    /// CHAP authentication state for target-to-initiator (if using Mutual CHAP)
    pub target_chap_state: Option<ChapAuthState>,
    /// Whether CHAP authentication has completed successfully (used to distinguish "never started" from "completed")
    pub chap_completed: bool,
    /// Access Control List - allowed initiator IQNs (None = allow all)
    pub allowed_initiators: Option<Vec<String>>,
}

impl Default for IscsiSession {
    fn default() -> Self {
        Self::new()
    }
}

impl IscsiSession {
    /// Create a new session
    pub fn new() -> Self {
        IscsiSession {
            isid: [0u8; 6],
            tsih: 0,
            cid: 0,
            session_type: SessionType::Normal,
            state: SessionState::Free,
            params: SessionParams::default(),
            exp_cmd_sn: 1,
            max_cmd_sn: 1,
            stat_sn: 0,
            current_stage: 0,
            next_stage: 0,
            pending_writes: HashMap::new(),
            next_ttt: 1, // TTT 0 is reserved for unsolicited data
            last_sense_data: None,
            auth_config: AuthConfig::None,
            chap_state: None,
            target_chap_state: None,
            chap_completed: false,
            allowed_initiators: None,
        }
    }

    /// Generate the next Target Transfer Tag
    pub fn next_target_transfer_tag(&mut self) -> u32 {
        let ttt = self.next_ttt;
        self.next_ttt = self.next_ttt.wrapping_add(1);
        // TTT 0xFFFFFFFF is reserved (means no TTT), so skip it
        if self.next_ttt == 0xFFFF_FFFF {
            self.next_ttt = 1;
        }
        ttt
    }

    /// Create session from login request
    pub fn from_login_request(login: &LoginRequest, target_name: &str) -> Self {
        let mut session = IscsiSession::new();
        session.isid = login.isid;
        session.cid = login.cid;
        session.exp_cmd_sn = login.cmd_sn;
        session.max_cmd_sn = login.cmd_sn + 1;
        session.current_stage = login.csg;
        session.next_stage = login.nsg;
        session.params.target_name = target_name.to_string();

        // Parse initiator parameters
        for (key, value) in &login.parameters {
            session.apply_initiator_param(key, value);
        }

        // Set initial state based on CSG
        session.state = match login.csg {
            0 => SessionState::SecurityNegotiation,
            1 => SessionState::LoginOperationalNegotiation,
            3 => SessionState::FullFeaturePhase,
            _ => SessionState::SecurityNegotiation,
        };

        session
    }

    /// Set authentication configuration for this session
    pub fn set_auth_config(&mut self, auth_config: AuthConfig) {
        self.auth_config = auth_config;
    }

    /// Set ACL (Access Control List) for this session
    pub fn set_allowed_initiators(&mut self, allowed_initiators: Option<Vec<String>>) {
        self.allowed_initiators = allowed_initiators;
    }

    /// Handle CHAP authentication during security negotiation
    /// Returns (success, response_params)
    fn handle_chap_auth(&mut self, login_params: &[(String, String)]) -> ScsiResult<(bool, Vec<(String, String)>)> {
        use crate::auth::parse_chap_response;

        // Check if initiator requested CHAP (may be "CHAP" or "CHAP,None" etc.)
        log::debug!("handle_chap_auth called with {} parameters", login_params.len());
        for (k, v) in login_params.iter() {
            log::debug!("  Login param: {}={}", k, v);
        }

        let auth_method = login_params.iter()
            .find(|(k, _)| k == "AuthMethod")
            .map(|(_, v)| v.as_str());

        log::debug!("AuthMethod parameter: {:?}", auth_method);

        // Check if CHAP is in the list of methods
        let supports_chap = auth_method.map(|m| m.contains("CHAP")).unwrap_or(false);
        log::debug!("supports_chap: {}", supports_chap);

        match &self.auth_config {
            AuthConfig::None => {
                // No auth required - accept None or CHAP
                // Only echo back AuthMethod=None if initiator sent AuthMethod in this PDU
                // This allows state transitions on subsequent PDUs that don't include AuthMethod
                if auth_method.is_some() {
                    Ok((true, vec![("AuthMethod".to_string(), "None".to_string())]))
                } else {
                    Ok((true, vec![]))
                }
            }
            AuthConfig::Chap { credentials } | AuthConfig::MutualChap { target_credentials: credentials, .. } => {
                // CHAP is required

                // Handle empty transit request after CHAP completes (Mutual CHAP only)
                // RFC 3720: After Mutual CHAP, initiator sends empty request with Transit=true
                if self.chap_completed && login_params.is_empty() {
                    log::debug!("CHAP already completed, allowing empty transit request for phase transition");
                    return Ok((true, vec![]));
                }

                // Check if initiator has selected an algorithm
                let chap_a = login_params.iter()
                    .find(|(k, _)| k == "CHAP_A")
                    .map(|(_, v)| v.as_str());

                // Allow CHAP continuation even if AuthMethod is not in current PDU
                // (only the first Login PDU contains AuthMethod)
                let chap_in_progress = self.chap_state.is_some() || chap_a.is_some();

                if supports_chap || chap_in_progress {
                    if chap_a.is_none() && self.chap_state.is_none() {
                        // Step 1: Acknowledge CHAP (initiator will request algorithm list next)
                        let params = vec![
                            ("TargetPortalGroupTag".to_string(), "1".to_string()),
                            ("AuthMethod".to_string(), "CHAP".to_string()),
                        ];
                        log::debug!("Acknowledging CHAP authentication method");
                        Ok((false, params))
                    } else if chap_a.is_some() && self.chap_state.is_none() {
                        // Step 2: Initiator requested algorithm (sends CHAP_A=5), send challenge
                        let chap_state = ChapAuthState::new(false);
                        let params = vec![
                            ("CHAP_A".to_string(), "5".to_string()), // Confirm MD5
                            ("CHAP_I".to_string(), chap_state.identifier_str()),
                            ("CHAP_C".to_string(), chap_state.challenge_hex()),
                        ];

                        // For mutual CHAP, we'll handle target auth after validating initiator
                        self.chap_state = Some(chap_state);

                        log::debug!("Sending CHAP challenge to initiator");
                        Ok((false, params)) // Not authenticated yet
                    } else if self.chap_state.is_some() {
                        // Second step: Validate initiator response
                        let chap_n = login_params.iter()
                            .find(|(k, _)| k == "CHAP_N")
                            .map(|(_, v)| v.as_str());
                        let chap_r = login_params.iter()
                            .find(|(k, _)| k == "CHAP_R")
                            .map(|(_, v)| v.as_str());

                        if let (Some(username), Some(response_hex)) = (chap_n, chap_r) {
                            // Validate username
                            if username != credentials.username {
                                log::warn!("CHAP authentication failed: unknown user '{}' (expected '{}')",
                                    username, credentials.username);
                                return Err(IscsiError::Auth(format!(
                                    "AUTH_FAILURE: Unknown user '{}' - check username in authentication credentials",
                                    username
                                )));
                            }

                            // Parse and validate response
                            let response = parse_chap_response(response_hex)?;
                            let chap_state = self.chap_state.as_ref().unwrap();

                            if chap_state.validate_response(&response, &credentials.secret) {
                                log::info!("CHAP authentication successful for user '{}'", username);

                                // TODO: Add ACL (Access Control List) check here to return AUTHORIZATION_FAILURE
                                // if the initiator is authenticated but not authorized for this target.
                                // Example:
                                // if !self.check_initiator_acl(&self.params.initiator_name, target_name) {
                                //     return Err(IscsiError::Auth(
                                //         "AUTHORIZATION_FAILURE: Initiator authenticated but not in target's ACL".to_string()
                                //     ));
                                // }

                                // Check if mutual CHAP is required
                                if let AuthConfig::MutualChap { initiator_credentials, .. } = &self.auth_config {
                                    // In mutual CHAP, initiator may send a challenge to target
                                    // Check if initiator sent CHAP_I and CHAP_C (target auth)
                                    let target_chap_i = login_params.iter()
                                        .find(|(k, _)| k == "CHAP_I")
                                        .map(|(_, v)| v.as_str());
                                    let target_chap_c = login_params.iter()
                                        .find(|(k, _)| k == "CHAP_C")
                                        .map(|(_, v)| v.as_str());

                                    if let (Some(chap_i), Some(chap_c_hex)) = (target_chap_i, target_chap_c) {
                                        // Initiator is challenging us - respond with initiator's credentials
                                        // (target proves its identity using credentials the initiator expects)
                                        log::debug!("Mutual CHAP: Received challenge from initiator (I={}, C={})", chap_i, &chap_c_hex[..20.min(chap_c_hex.len())]);

                                        // Parse challenge
                                        let identifier = chap_i.parse::<u8>().map_err(|e|
                                            IscsiError::Auth(format!("Invalid CHAP_I: {}", e)))?;

                                        // Remove "0x" prefix if present
                                        let chap_c_clean = chap_c_hex.strip_prefix("0x").unwrap_or(chap_c_hex);
                                        let challenge = hex::decode(chap_c_clean).map_err(|e|
                                            IscsiError::Auth(format!("Invalid CHAP_C hex: {}", e)))?;

                                        // Calculate target's response using initiator_credentials
                                        // (these are the credentials the initiator expects from the target)
                                        let mut data = Vec::new();
                                        data.push(identifier);
                                        data.extend_from_slice(initiator_credentials.secret.as_bytes());
                                        data.extend_from_slice(&challenge);
                                        let target_response = md5::compute(&data).0.to_vec();

                                        let response_hex = format!("0x{}", target_response.iter()
                                            .map(|b| format!("{:02x}", b))
                                            .collect::<String>());

                                        let params = vec![
                                            ("CHAP_N".to_string(), initiator_credentials.username.clone()),
                                            ("CHAP_R".to_string(), response_hex),
                                        ];

                                        log::info!("Mutual CHAP: Both parties authenticated successfully");

                                        // Clear CHAP state to indicate authentication is complete
                                        // The next login request will not have CHAP parameters
                                        self.chap_state = None;
                                        self.chap_completed = true;

                                        return Ok((true, params)); // Send target's response and complete auth
                                    }
                                }

                                // Clear CHAP state after successful one-way CHAP
                                self.chap_state = None;
                                self.chap_completed = true;
                                Ok((true, vec![])) // Authenticated successfully (one-way CHAP)
                            } else {
                                log::warn!("CHAP authentication failed: invalid password/secret for user '{}'", username);
                                Err(IscsiError::Auth(format!(
                                    "AUTH_FAILURE: Invalid password for user '{}' - CHAP response does not match expected value",
                                    username
                                )))
                            }
                        } else {
                            log::warn!("CHAP authentication failed: missing CHAP_N or CHAP_R parameters");
                            Err(IscsiError::Auth(
                                "AUTH_FAILURE: Missing CHAP_N (username) or CHAP_R (response) - initiator must provide credentials".to_string()
                            ))
                        }
                    } else {
                        // Unexpected state
                        log::warn!("CHAP authentication: unexpected state");
                        Err(IscsiError::Auth("CHAP authentication protocol error".to_string()))
                    }
                } else {
                    // Initiator must use CHAP but didn't request it
                    log::warn!("Authentication required but initiator didn't request CHAP (AuthMethod missing or wrong)");
                    Err(IscsiError::Auth(
                        "AUTH_FAILURE: CHAP authentication required but initiator did not request it - set AuthMethod=CHAP".to_string()
                    ))
                }
            }
        }
    }

    /// Apply an initiator parameter during negotiation
    fn apply_initiator_param(&mut self, key: &str, value: &str) {
        match key {
            "InitiatorName" => {
                self.params.initiator_name = value.to_string();
            }
            "InitiatorAlias" => {
                self.params.initiator_alias = value.to_string();
            }
            "TargetName" => {
                // Initiator requests specific target
                self.params.target_name = value.to_string();
            }
            "SessionType" => {
                // RFC 3720: Only "Discovery" and "Normal" are valid
                match value {
                    "Discovery" => {
                        self.session_type = SessionType::Discovery;
                    }
                    "Normal" => {
                        self.session_type = SessionType::Normal;
                    }
                    _ => {
                        log::warn!("Invalid SessionType '{}' - only 'Discovery' and 'Normal' are supported", value);
                        // Store invalid session type to reject during login processing
                        self.params.invalid_session_type = Some(value.to_string());
                    }
                }
            }
            "MaxRecvDataSegmentLength" => {
                if let Ok(v) = value.parse::<u32>() {
                    // This is initiator's max recv, which is our max xmit
                    self.params.max_xmit_data_segment_length = v;
                }
            }
            "MaxBurstLength" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.params.max_burst_length = v.min(self.params.max_burst_length);
                    // RFC 3720 12.14: FirstBurstLength MUST NOT exceed MaxBurstLength.
                    self.params.first_burst_length =
                        self.params.first_burst_length.min(self.params.max_burst_length);
                }
            }
            "FirstBurstLength" => {
                if let Ok(v) = value.parse::<u32>() {
                    // RFC 3720 12.14: Minimum, and MUST NOT exceed MaxBurstLength.
                    self.params.first_burst_length = v
                        .min(self.params.first_burst_length)
                        .min(self.params.max_burst_length);
                }
            }
            "DefaultTime2Wait" => {
                if let Ok(v) = value.parse::<u16>() {
                    self.params.default_time2wait = v.max(self.params.default_time2wait);
                }
            }
            "DefaultTime2Retain" => {
                if let Ok(v) = value.parse::<u16>() {
                    self.params.default_time2retain = v.min(self.params.default_time2retain);
                }
            }
            "MaxOutstandingR2T" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.params.max_outstanding_r2t = v.min(self.params.max_outstanding_r2t);
                }
            }
            "DataPDUInOrder" => {
                // RFC 3720 12.18: boolean result function OR.
                self.params.data_pdu_in_order =
                    self.params.data_pdu_in_order || (value == "Yes");
            }
            "DataSequenceInOrder" => {
                // RFC 3720 12.19: boolean result function OR.
                self.params.data_sequence_in_order =
                    self.params.data_sequence_in_order || (value == "Yes");
            }
            "ErrorRecoveryLevel" => {
                if let Ok(v) = value.parse::<u8>() {
                    self.params.error_recovery_level = v.min(self.params.error_recovery_level);
                }
            }
            "ImmediateData" => {
                // AND operation: only true if both want it
                self.params.immediate_data = self.params.immediate_data && (value == "Yes");
            }
            "InitialR2T" => {
                // OR operation: true if either wants it
                self.params.initial_r2t = self.params.initial_r2t || (value == "Yes");
            }
            "HeaderDigest" => {
                self.params.header_digest = negotiate_digest(value);
            }
            "DataDigest" => {
                self.params.data_digest = negotiate_digest(value);
            }
            // Authentication parameters - handled separately in handle_chap_auth()
            "AuthMethod" | "CHAP_A" | "CHAP_I" | "CHAP_C" | "CHAP_N" | "CHAP_R" => {
                // These are processed by handle_chap_auth, not here
            }
            _ => {
                // Unknown parameter - ignore
                log::debug!("Ignoring unknown parameter: {}={}", key, value);
            }
        }
    }

    /// Generate target response parameters for login
    pub fn generate_response_params(&self) -> Vec<(String, String)> {
        let mut params = Vec::new();

        // Note: SessionType and TargetName are declarative (initiator-only) and should NOT be echoed back
        // Only send TargetAlias if configured
        if self.session_type == SessionType::Normal {
            if !self.params.target_alias.is_empty() {
                params.push(("TargetAlias".to_string(), self.params.target_alias.clone()));
            }
        }

        // Negotiated parameters
        params.push((
            "MaxRecvDataSegmentLength".to_string(),
            self.params.max_recv_data_segment_length.to_string(),
        ));
        params.push((
            "MaxBurstLength".to_string(),
            self.params.max_burst_length.to_string(),
        ));
        params.push((
            "FirstBurstLength".to_string(),
            self.params.first_burst_length.to_string(),
        ));
        params.push((
            "DefaultTime2Wait".to_string(),
            self.params.default_time2wait.to_string(),
        ));
        params.push((
            "DefaultTime2Retain".to_string(),
            self.params.default_time2retain.to_string(),
        ));
        params.push((
            "MaxOutstandingR2T".to_string(),
            self.params.max_outstanding_r2t.to_string(),
        ));
        params.push((
            "DataPDUInOrder".to_string(),
            if self.params.data_pdu_in_order { "Yes" } else { "No" }.to_string(),
        ));
        params.push((
            "DataSequenceInOrder".to_string(),
            if self.params.data_sequence_in_order { "Yes" } else { "No" }.to_string(),
        ));
        params.push((
            "ErrorRecoveryLevel".to_string(),
            self.params.error_recovery_level.to_string(),
        ));
        params.push((
            "ImmediateData".to_string(),
            if self.params.immediate_data { "Yes" } else { "No" }.to_string(),
        ));
        params.push((
            "InitialR2T".to_string(),
            if self.params.initial_r2t { "Yes" } else { "No" }.to_string(),
        ));
        params.push((
            "HeaderDigest".to_string(),
            match self.params.header_digest {
                DigestType::None => "None",
                DigestType::CRC32C => "CRC32C",
            }.to_string(),
        ));
        params.push((
            "DataDigest".to_string(),
            match self.params.data_digest {
                DigestType::None => "None",
                DigestType::CRC32C => "CRC32C",
            }.to_string(),
        ));

        params
    }

    /// Process a login request and generate response
    pub fn process_login(&mut self, pdu: &IscsiPdu, target_name: &str) -> ScsiResult<IscsiPdu> {
        let login = pdu.parse_login_request()?;

        // Check iSCSI version compatibility - RFC 3720 Section 11.12
        // Target supports version 0x00 (RFC 3720)
        const TARGET_VERSION: u8 = 0x00;
        if TARGET_VERSION < login.version_min || TARGET_VERSION > login.version_max {
            log::warn!(
                "Login rejected: version mismatch (initiator: min=0x{:02x}, max=0x{:02x}, target=0x{:02x})",
                login.version_min, login.version_max, TARGET_VERSION
            );
            return self.create_unsupported_version_reject(pdu.itt, login.version_max, login.version_min);
        }

        // First login - initialize session
        if self.state == SessionState::Free {
            self.isid = login.isid;
            self.cid = login.cid;
            self.exp_cmd_sn = login.cmd_sn;
            self.max_cmd_sn = login.cmd_sn + 1;
            self.params.target_name = target_name.to_string();
        }

        // Apply parameters from this login PDU
        log::debug!("Received {} login parameters: {:?}", login.parameters.len(), login.parameters);
        for (key, value) in &login.parameters {
            self.apply_initiator_param(key, value);
        }

        // Validate required parameters - RFC 3720 Section 12
        let has_initiator_name = login.parameters.iter()
            .any(|(k, _)| k == "InitiatorName");

        // InitiatorName is required, but only if we haven't already received it
        // iscsiadm sends it in the first login PDU, then sends follow-up PDUs without it
        if !has_initiator_name && self.params.initiator_name.is_empty() {
            log::warn!("Login rejected: missing required InitiatorName parameter");
            return self.create_login_reject(
                pdu.itt,
                pdu::login_status::INITIATOR_ERROR,
                0x07, // Missing parameter (MISSING_PARAMETER from pdu.rs)
            );
        }

        // Validate target name for normal sessions
        if self.session_type == SessionType::Normal {
            let requested_target = login.parameters.iter()
                .find(|(k, _)| k == "TargetName")
                .map(|(_, v)| v.as_str());

            // TargetName is required for normal sessions, but only on the first login PDU
            // After that, iscsiadm (and other initiators) may send login PDUs without TargetName
            // We detect first PDU by checking if ISID has been set yet (it's [0,0,0,0,0,0] initially)
            let is_first_login = self.isid == [0u8; 6];
            if requested_target.is_none() && is_first_login {
                log::warn!("Login rejected: missing required TargetName parameter for normal session");
                return self.create_login_reject(
                    pdu.itt,
                    pdu::login_status::INITIATOR_ERROR,
                    0x07, // Missing parameter
                );
            }

            // If TargetName is provided in this PDU, validate it matches our target
            if let Some(req_name) = requested_target {
                if req_name != target_name {
                    log::warn!("Login rejected: target '{}' not found (have: '{}')", req_name, target_name);
                    return self.create_login_reject(
                        pdu.itt,
                        pdu::login_status::INITIATOR_ERROR,
                        0x03, // Target not found (TARGET_NOT_FOUND from pdu.rs)
                    );
                }
            }
        }

        // Validate SessionType - only "Discovery" and "Normal" are supported (RFC 3720)
        if let Some(ref invalid_type) = self.params.invalid_session_type {
            log::warn!("Login rejected: unsupported SessionType '{}' (only 'Discovery' and 'Normal' are valid)", invalid_type);
            return self.create_login_reject(
                pdu.itt,
                pdu::login_status::INITIATOR_ERROR,
                0x09, // SESSION_TYPE_NOT_SUPPORTED (0x0209)
            );
        }

        // Update stages
        self.current_stage = login.csg;
        self.next_stage = login.nsg;

        log::debug!("Login: CSG={}, NSG={}, Transit={}", login.csg, login.nsg, login.transit);

        // Handle authentication during security negotiation (CSG=0)
        // IMPORTANT: Check auth BEFORE deciding whether to honor transit request
        let auth_complete = if login.csg == 0 {
            // Handle auth errors by returning login reject PDU instead of propagating error
            let (auth_success, auth_params) = match self.handle_chap_auth(&login.parameters) {
                Ok((success, params)) => (success, params),
                Err(e) => {
                    // Auth error - send login reject with AUTH_FAILURE status
                    log::warn!("Login rejected: {}", e);
                    return self.create_login_reject(
                        pdu.itt,
                        pdu::login_status::INITIATOR_ERROR,
                        0x01, // AUTH_FAILURE (0x0201)
                    );
                }
            };

            log::debug!("After handle_chap_auth: auth_success={}, auth_params={:?}", auth_success, auth_params);

            // If authentication in progress, send CHAP parameters and stay in security negotiation
            // OR if mutual CHAP completed successfully and we need to send target's response
            if !auth_params.is_empty() {
                // Send CHAP challenge/response
                let response_data = serialize_text_parameters(&auth_params);

                log::debug!("Sending {} auth parameters: {:?}", auth_params.len(), auth_params);

                self.stat_sn = self.stat_sn.wrapping_add(1);

                // For mutual CHAP, DO NOT set transit bit even if initiator sets it
                // RFC 3720: Mutual CHAP requires the target to send its response
                // WITHOUT transitioning, then the initiator will send another login
                // request to complete the phase transition
                let (csg, nsg, transit) = (0, 0, false);

                return Ok(IscsiPdu::login_response(
                    self.isid,
                    self.tsih,
                    self.stat_sn,
                    self.exp_cmd_sn,
                    self.max_cmd_sn,
                    0, // status_class: success
                    0, // status_detail: success
                    csg, // CSG: Security Negotiation
                    nsg, // NSG: depends on whether auth is complete
                    transit, // transit: depends on whether auth is complete
                    pdu.itt,
                    response_data,
                ));
            }

            // If authentication required but failed with error, reject the login
            if !auth_success {
                log::warn!("Login rejected: authentication failed");
                return self.create_login_reject(
                    pdu.itt,
                    pdu::login_status::INITIATOR_ERROR,
                    0x01, // AUTH_FAILURE (from pdu.rs)
                );
            }

            // Authentication successful
            auth_success
        } else {
            // Not in security negotiation, auth not required
            true
        };

        // Check ACL (Access Control List) after authentication succeeds
        // This check happens once per session during the first login PDU with auth complete
        if auth_complete && self.state == SessionState::Free {
            if let Some(ref allowed) = self.allowed_initiators {
                let initiator_name = &self.params.initiator_name;
                if !allowed.contains(initiator_name) {
                    log::warn!(
                        "Login rejected: initiator '{}' not in ACL (allowed: {:?})",
                        initiator_name, allowed
                    );
                    return self.create_authorization_failure_reject(pdu.itt);
                }
                log::debug!("ACL check passed for initiator '{}'", initiator_name);
            }
        }

        // Determine response transit flags
        // Only allow transit if authentication is complete (or not required)
        let transit = login.transit && auth_complete;
        log::debug!("Transition logic: login.transit={}, auth_complete={}, transit={}",
            login.transit, auth_complete, transit);
        let (response_csg, response_nsg, response_transit) = if transit {
            // Initiator wants to transition and auth is complete
            log::debug!("Checking transition: CSG={}, NSG={}", login.csg, login.nsg);
            match (login.csg, login.nsg) {
                (0, 1) => {
                    // Security → Login Op Neg
                    self.state = SessionState::LoginOperationalNegotiation;
                    (login.csg, login.nsg, true) // Echo back the transition
                }
                (0, 3) => {
                    // Security → Full Feature Phase
                    self.state = SessionState::FullFeaturePhase;
                    // Only assign TSIH for Normal sessions, not Discovery
                    if self.session_type == SessionType::Normal {
                        self.tsih = self.generate_tsih();
                    }
                    (login.csg, login.nsg, true) // Echo back the transition
                }
                (1, 3) => {
                    // Login Op Neg → Full Feature Phase
                    self.state = SessionState::FullFeaturePhase;
                    // Only assign TSIH for Normal sessions, not Discovery
                    if self.session_type == SessionType::Normal {
                        self.tsih = self.generate_tsih();
                    }
                    (login.csg, login.nsg, true) // Echo back the transition
                }
                _ => {
                    // Stay in current stage
                    (login.csg, login.nsg, false)
                }
            }
        } else {
            // Initiator not ready to transition, or auth not complete
            (login.csg, login.nsg, false)
        };

        log::debug!("Response: CSG={}, NSG={}, Transit={}", response_csg, response_nsg, response_transit);

        // Generate response parameters
        let response_params = if response_transit && response_nsg == 3 {
            // Final login response
            if self.session_type == SessionType::Discovery {
                // Discovery sessions - only echo back operational parameters
                let mut params = vec![];

                // Include operational parameters that were negotiated
                for (key, _value) in &login.parameters {
                    match key.as_str() {
                        "MaxRecvDataSegmentLength" => {
                            params.push(("MaxRecvDataSegmentLength".to_string(),
                                        self.params.max_recv_data_segment_length.to_string()));
                        }
                        "HeaderDigest" => {
                            params.push(("HeaderDigest".to_string(),
                                match self.params.header_digest {
                                    DigestType::CRC32C => "CRC32C",
                                    DigestType::None => "None",
                                }.to_string()));
                        }
                        "DataDigest" => {
                            params.push(("DataDigest".to_string(),
                                match self.params.data_digest {
                                    DigestType::CRC32C => "CRC32C",
                                    DigestType::None => "None",
                                }.to_string()));
                        }
                        _ => {}
                    }
                }
                params
            } else {
                // Normal sessions get full parameter negotiation
                self.generate_response_params()
            }
        } else {
            // Intermediate response
            vec![]
        };

        let response_data = serialize_text_parameters(&response_params);

        log::debug!("Sending {} response parameters: {:?}", response_params.len(), response_params);
        log::debug!("Response data ({} bytes): {:?}", response_data.len(), String::from_utf8_lossy(&response_data));

        // Increment stat_sn for this response
        self.stat_sn = self.stat_sn.wrapping_add(1);

        Ok(IscsiPdu::login_response(
            self.isid,
            self.tsih,
            self.stat_sn,
            self.exp_cmd_sn,
            self.max_cmd_sn,
            pdu::login_status::SUCCESS,
            0, // status detail
            response_csg,
            response_nsg,
            response_transit,
            pdu.itt,
            response_data,
        ))
    }

    /// Create a login reject response
    fn create_login_reject(&self, itt: u32, status_class: u8, status_detail: u8) -> ScsiResult<IscsiPdu> {
        Ok(IscsiPdu::login_response(
            self.isid,
            0, // No TSIH for reject
            self.stat_sn,
            self.exp_cmd_sn,
            self.max_cmd_sn,
            status_class,
            status_detail,
            self.current_stage,
            self.next_stage,
            false, // No transit on error
            itt,
            Vec::new(),
        ))
    }

    /// Create a login reject for graceful shutdown - RFC 3720: SERVICE_UNAVAILABLE (0x0301)
    ///
    /// This is used when the target is gracefully shutting down and should reject new login
    /// attempts while allowing existing sessions to complete their work.
    pub fn create_shutdown_reject(&self, itt: u32) -> ScsiResult<IscsiPdu> {
        log::info!("Rejecting login attempt during graceful shutdown with SERVICE_UNAVAILABLE");
        self.create_login_reject(
            itt,
            pdu::login_status::TARGET_ERROR,
            0x01, // SERVICE_UNAVAILABLE (0x0301)
        )
    }

    /// Create a login reject for connection limit - RFC 3720: TOO_MANY_CONNECTIONS (0x0206)
    ///
    /// This is used when the target has reached its maximum number of concurrent connections
    /// and cannot accept new login attempts.
    pub fn create_too_many_connections_reject(&self, itt: u32) -> ScsiResult<IscsiPdu> {
        log::info!("Rejecting login attempt due to connection limit with TOO_MANY_CONNECTIONS");
        self.create_login_reject(
            itt,
            pdu::login_status::INITIATOR_ERROR,
            0x06, // TOO_MANY_CONNECTIONS (0x0206)
        )
    }

    /// Create a login reject for invalid request - RFC 3720: INVALID_REQUEST_DURING_LOGIN (0x020B)
    ///
    /// This is used when an initiator sends a PDU that is not allowed during the login phase,
    /// such as a SCSI command before login completes or an invalid opcode.
    pub fn create_invalid_request_during_login_reject(&self, itt: u32) -> ScsiResult<IscsiPdu> {
        log::warn!("Rejecting login due to invalid request during login phase (INVALID_REQUEST_DURING_LOGIN)");
        self.create_login_reject(
            itt,
            pdu::login_status::INITIATOR_ERROR,
            0x0B, // INVALID_REQUEST_DURING_LOGIN (0x020B)
        )
    }

    /// Create a login reject for unsupported version - RFC 3720: UNSUPPORTED_VERSION (0x0205)
    ///
    /// This is used when the initiator's version range doesn't include the version supported by the target.
    /// The target supports iSCSI version 0x00 (RFC 3720).
    pub fn create_unsupported_version_reject(&self, itt: u32, version_max: u8, version_min: u8) -> ScsiResult<IscsiPdu> {
        log::warn!(
            "Rejecting login due to unsupported version (initiator: max={}, min={}, target: 0x00)",
            version_max, version_min
        );
        self.create_login_reject(
            itt,
            pdu::login_status::INITIATOR_ERROR,
            0x05, // UNSUPPORTED_VERSION (0x0205)
        )
    }

    /// Create a login reject for authorization failure - RFC 3720: AUTHORIZATION_FAILURE (0x0202)
    ///
    /// This is used when authentication succeeds but the initiator is not authorized
    /// to access the target (ACL check fails).
    pub fn create_authorization_failure_reject(&self, itt: u32) -> ScsiResult<IscsiPdu> {
        log::warn!("Rejecting login due to authorization failure (ACL check failed)");
        self.create_login_reject(
            itt,
            pdu::login_status::INITIATOR_ERROR,
            0x02, // AUTHORIZATION_FAILURE (0x0202)
        )
    }

    /// Create a login reject for resource exhaustion - RFC 3720: OUT_OF_RESOURCES (0x0302)
    ///
    /// This is used when the target cannot process the login due to resource constraints
    /// such as memory allocation failures or session limits.
    pub fn create_out_of_resources_reject(&self, itt: u32) -> ScsiResult<IscsiPdu> {
        log::error!("Rejecting login due to resource exhaustion (OUT_OF_RESOURCES)");
        self.create_login_reject(
            itt,
            pdu::login_status::TARGET_ERROR,
            0x02, // OUT_OF_RESOURCES (0x0302)
        )
    }

    /// Generate a unique TSIH
    fn generate_tsih(&self) -> u16 {
        // Simple TSIH generation - in production, would be globally unique
        use std::time::{SystemTime, UNIX_EPOCH};
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        ((duration.as_millis() & 0xFFFF) as u16).max(1)
    }

    /// Check if session is in full feature phase
    pub fn is_full_feature(&self) -> bool {
        self.state == SessionState::FullFeaturePhase
    }

    /// Check if this is a discovery session
    pub fn is_discovery(&self) -> bool {
        self.session_type == SessionType::Discovery
    }

    /// Get next StatSN and increment
    pub fn next_stat_sn(&mut self) -> u32 {
        let sn = self.stat_sn;
        self.stat_sn = self.stat_sn.wrapping_add(1);
        sn
    }

    /// Validate and update CmdSN from incoming PDU
    pub fn validate_cmd_sn(&mut self, cmd_sn: u32) -> bool {
        // Check if CmdSN is within window
        let in_window = Self::sn_in_window(cmd_sn, self.exp_cmd_sn, self.max_cmd_sn);

        if in_window && cmd_sn == self.exp_cmd_sn {
            // Expected command - advance window
            self.exp_cmd_sn = self.exp_cmd_sn.wrapping_add(1);
            self.max_cmd_sn = self.max_cmd_sn.wrapping_add(1);
        }

        in_window
    }

    /// Check if a sequence number is within the command window
    fn sn_in_window(sn: u32, exp_sn: u32, max_sn: u32) -> bool {
        // Handle wraparound using signed comparison
        let diff_exp = sn.wrapping_sub(exp_sn) as i32;
        let diff_max = max_sn.wrapping_sub(sn) as i32;
        diff_exp >= 0 && diff_max >= 0
    }

    /// Process logout request
    pub fn process_logout(&mut self, pdu: &IscsiPdu) -> ScsiResult<IscsiPdu> {
        let logout = pdu.parse_logout_request()?;

        self.state = SessionState::Logout;

        Ok(IscsiPdu::logout_response(
            logout.itt,
            self.next_stat_sn(),
            self.exp_cmd_sn,
            self.max_cmd_sn,
            pdu::logout_response::SUCCESS,
            self.params.default_time2wait,
            self.params.default_time2retain,
        ))
    }

    /// Process NOP-Out (ping) request
    pub fn process_nop_out(&mut self, pdu: &IscsiPdu) -> ScsiResult<IscsiPdu> {
        let nop = pdu.parse_nop_out()?;

        // Only respond if ITT is not 0xFFFFFFFF (unsolicited NOP-In)
        if nop.itt == 0xFFFF_FFFF {
            // Target-initiated ping, initiator responding - no response needed
            return Err(IscsiError::Protocol("Unsolicited NOP-Out response".to_string()));
        }

        Ok(IscsiPdu::nop_in(
            nop.itt,
            0xFFFF_FFFF, // TTT for response
            self.next_stat_sn(),
            self.exp_cmd_sn,
            self.max_cmd_sn,
            nop.lun,
        ))
    }

    /// Handle SendTargets discovery request
    pub fn handle_send_targets(&self, target_name: &str, target_address: &str) -> Vec<(String, String)> {
        vec![
            ("TargetName".to_string(), target_name.to_string()),
            ("TargetAddress".to_string(), format!("{},1", target_address)),
        ]
    }
}

/// Connection state for a single TCP connection within a session
#[derive(Debug, Clone)]
pub struct IscsiConnection {
    /// Connection ID
    pub cid: u16,
    /// Connection state
    pub state: ConnectionState,
    /// Associated session
    pub session_id: Option<u16>, // TSIH
}

/// Connection states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum ConnectionState {
    /// Initial state
    #[default]
    Free,
    /// In login phase
    InLogin,
    /// Logged in, full feature phase
    LoggedIn,
    /// In logout phase
    InLogout,
    /// Cleanup state
    Cleanup,
}


impl IscsiConnection {
    /// Create a new connection
    pub fn new(cid: u16) -> Self {
        IscsiConnection {
            cid,
            state: ConnectionState::Free,
            session_id: None,
        }
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let session = IscsiSession::new();
        assert_eq!(session.state, SessionState::Free);
        assert_eq!(session.tsih, 0);
        assert_eq!(session.exp_cmd_sn, 1);
    }

    #[test]
    fn test_session_params_default() {
        let params = SessionParams::default();
        assert_eq!(params.max_recv_data_segment_length, 8192);
        assert_eq!(params.max_burst_length, 262144);
        assert_eq!(params.first_burst_length, 65536);
        assert_eq!(params.error_recovery_level, 0);
        assert!(params.data_pdu_in_order);
        assert!(params.data_sequence_in_order);
    }

    #[test]
    fn test_session_type() {
        let mut session = IscsiSession::new();
        assert_eq!(session.session_type, SessionType::Normal);

        session.apply_initiator_param("SessionType", "Discovery");
        assert_eq!(session.session_type, SessionType::Discovery);

        session.apply_initiator_param("SessionType", "Normal");
        assert_eq!(session.session_type, SessionType::Normal);
    }

    #[test]
    fn test_parameter_negotiation() {
        let mut session = IscsiSession::new();

        // Test MaxBurstLength - should take minimum
        session.params.max_burst_length = 262144;
        session.apply_initiator_param("MaxBurstLength", "131072");
        assert_eq!(session.params.max_burst_length, 131072);

        // Test InitiatorName
        session.apply_initiator_param("InitiatorName", "iqn.2025-12.test:initiator");
        assert_eq!(session.params.initiator_name, "iqn.2025-12.test:initiator");

        // Test ErrorRecoveryLevel - should take minimum
        session.params.error_recovery_level = 2;
        session.apply_initiator_param("ErrorRecoveryLevel", "1");
        assert_eq!(session.params.error_recovery_level, 1);
    }

    #[test]
    fn test_immediate_data_negotiation() {
        let mut session = IscsiSession::new();

        // Both want it - should be true
        session.params.immediate_data = true;
        session.apply_initiator_param("ImmediateData", "Yes");
        assert!(session.params.immediate_data);

        // Target wants it, initiator doesn't - should be false
        session.params.immediate_data = true;
        session.apply_initiator_param("ImmediateData", "No");
        assert!(!session.params.immediate_data);
    }

    #[test]
    fn test_initial_r2t_negotiation() {
        let mut session = IscsiSession::new();

        // Both don't want it - should be false
        session.params.initial_r2t = false;
        session.apply_initiator_param("InitialR2T", "No");
        assert!(!session.params.initial_r2t);

        // One wants it - should be true (OR operation)
        session.params.initial_r2t = false;
        session.apply_initiator_param("InitialR2T", "Yes");
        assert!(session.params.initial_r2t);
    }

    #[test]
    fn test_sequence_number_validation() {
        let mut session = IscsiSession::new();
        session.exp_cmd_sn = 100;
        session.max_cmd_sn = 110;

        // In window - valid
        assert!(session.validate_cmd_sn(100));
        assert_eq!(session.exp_cmd_sn, 101); // Should advance

        // Also in window
        assert!(session.validate_cmd_sn(105));

        // Out of window - invalid
        assert!(!session.validate_cmd_sn(50));
        assert!(!session.validate_cmd_sn(200));
    }

    #[test]
    fn test_stat_sn_increment() {
        let mut session = IscsiSession::new();
        session.stat_sn = 0;

        assert_eq!(session.next_stat_sn(), 0);
        assert_eq!(session.stat_sn, 1);

        assert_eq!(session.next_stat_sn(), 1);
        assert_eq!(session.stat_sn, 2);
    }

    #[test]
    fn test_generate_response_params() {
        let mut session = IscsiSession::new();
        session.params.target_name = "iqn.2025-12.local:storage".to_string();
        session.params.max_recv_data_segment_length = 8192;

        let params = session.generate_response_params();

        // Check that required params are present
        // Note: SessionType should NOT be in response (it's initiator-only per RFC 3720)
        assert!(params.iter().any(|(k, _)| k == "MaxRecvDataSegmentLength"));
        assert!(params.iter().any(|(k, _)| k == "MaxBurstLength"));
    }

    #[test]
    fn test_session_states() {
        let mut session = IscsiSession::new();

        assert!(!session.is_full_feature());
        assert!(!session.is_discovery());

        session.state = SessionState::FullFeaturePhase;
        assert!(session.is_full_feature());

        session.session_type = SessionType::Discovery;
        assert!(session.is_discovery());
    }

    #[test]
    fn test_connection_new() {
        let conn = IscsiConnection::new(1);
        assert_eq!(conn.cid, 1);
        assert_eq!(conn.state, ConnectionState::Free);
        assert!(conn.session_id.is_none());
    }

    #[test]
    fn test_digest_type_default() {
        assert_eq!(DigestType::default(), DigestType::None);
    }

    #[test]
    fn test_send_targets() {
        let session = IscsiSession::new();
        let targets = session.handle_send_targets(
            "iqn.2025-12.local:storage",
            "192.168.1.100:3260"
        );

        assert_eq!(targets.len(), 2);
        assert!(targets.iter().any(|(k, v)| k == "TargetName" && v == "iqn.2025-12.local:storage"));
        assert!(targets.iter().any(|(k, v)| k == "TargetAddress" && v == "192.168.1.100:3260,1"));
    }

    #[test]
    fn test_header_digest_negotiation() {
        let mut session = IscsiSession::new();

        session.apply_initiator_param("HeaderDigest", "None");
        assert_eq!(session.params.header_digest, DigestType::None);

        session.apply_initiator_param("HeaderDigest", "CRC32C");
        assert_eq!(session.params.header_digest, DigestType::CRC32C);

        // Lists: first supported value wins, in the initiator's preference
        // order (RFC 3720 5.2.2)
        session.params.header_digest = DigestType::CRC32C;
        session.apply_initiator_param("HeaderDigest", "None,CRC32C");
        assert_eq!(session.params.header_digest, DigestType::None);

        session.params.header_digest = DigestType::None;
        session.apply_initiator_param("HeaderDigest", "CRC32C,None");
        assert_eq!(session.params.header_digest, DigestType::CRC32C);
    }
}
