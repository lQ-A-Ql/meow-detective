//! Typestate-based iSCSI session management
//!
//! This module uses Rust's type system to enforce valid state transitions at compile time.
//! Invalid state transitions become compile errors rather than runtime errors.
//!
//! ## State Machine (RFC 3720 Section 5)
//! ```text
//! Free --> SecurityNegotiation --> LoginOperationalNegotiation --> FullFeaturePhase --> Logout
//!                    |                                                    |
//!                    +---------------> FullFeaturePhase -----------------+
//!                                           |
//!                                           v
//!                                        Failed
//! ```
//!
//! ## Security Verification
//!
//! The typestate pattern provides **compile-time** enforcement of the login state machine:
//!
//! 1. **Private `_state` field**: The `TypestateSession<S>` struct has a private `_state: PhantomData<S>`
//!    field, which prevents external code from directly constructing sessions in arbitrary states.
//!    Attempting to bypass authentication by directly creating `TypestateSession<FullFeaturePhase>`
//!    results in: `error[E0451]: field '_state' of struct 'TypestateSession' is private`
//!
//! 2. **State-specific methods**: Methods like `process_nop_out()` and `process_logout()` only exist
//!    on `TypestateSession<FullFeaturePhase>`. Calling them on other states is a compile-time error.
//!
//! 3. **Sealed trait pattern**: The `SessionState` trait uses a private `Sealed` supertrait,
//!    preventing external code from implementing custom states.
//!
//! 4. **Consuming transitions**: State transitions consume `self` and return new typed sessions,
//!    ensuring the old state cannot be reused after a transition.
//!
//! ### AnySession Runtime Wrapper
//!
//! The `AnySession` enum provides runtime dispatch for cases where dynamic state handling is needed
//! (e.g., storing sessions in collections). While this adds runtime checks, it maintains safety by:
//! - Returning `Err` for invalid operations rather than allowing undefined behavior
//! - Delegating to the type-safe `TypestateSession<S>` methods internally

use crate::auth::{parse_chap_response, AuthConfig, ChapAuthState};
use crate::error::{IscsiError, ScsiResult};
use crate::pdu::{self, IscsiPdu, serialize_text_parameters};
use crate::session::{DigestType, PendingWrite, SessionParams, SessionType};
use std::collections::HashMap;
use std::marker::PhantomData;

// =============================================================================
// State Types (Zero-sized types used as type parameters)
// =============================================================================

/// Session is in Free state - waiting for first login PDU
pub struct Free;

/// Session is in Security Negotiation phase (CHAP, etc.)
pub struct SecurityNegotiation;

/// Session is in Login Operational Negotiation phase
pub struct LoginOperationalNegotiation;

/// Session is in Full Feature Phase - ready for SCSI commands
pub struct FullFeaturePhase;

/// Session is in Logout state
pub struct Logout;

/// Session is in Failed state
pub struct Failed;

// =============================================================================
// Sealed trait to prevent external implementations
// =============================================================================

mod private {
    pub trait Sealed {}
    impl Sealed for super::Free {}
    impl Sealed for super::SecurityNegotiation {}
    impl Sealed for super::LoginOperationalNegotiation {}
    impl Sealed for super::FullFeaturePhase {}
    impl Sealed for super::Logout {}
    impl Sealed for super::Failed {}
}

/// Marker trait for valid session states
pub trait SessionState: private::Sealed + std::fmt::Debug {}
impl SessionState for Free {}
impl SessionState for SecurityNegotiation {}
impl SessionState for LoginOperationalNegotiation {}
impl SessionState for FullFeaturePhase {}
impl SessionState for Logout {}
impl SessionState for Failed {}

impl std::fmt::Debug for Free {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Free")
    }
}
impl std::fmt::Debug for SecurityNegotiation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecurityNegotiation")
    }
}
impl std::fmt::Debug for LoginOperationalNegotiation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LoginOperationalNegotiation")
    }
}
impl std::fmt::Debug for FullFeaturePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FullFeaturePhase")
    }
}
impl std::fmt::Debug for Logout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Logout")
    }
}
impl std::fmt::Debug for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed")
    }
}

// =============================================================================
// Core Session Data (shared across all states)
// =============================================================================

/// Core session data shared across all states
#[derive(Debug, Clone)]
pub struct SessionData {
    /// Initiator Session ID (6 bytes)
    pub isid: [u8; 6],
    /// Target Session Identifying Handle
    pub tsih: u16,
    /// Connection ID
    pub cid: u16,
    /// Session type
    pub session_type: SessionType,
    /// Negotiated parameters
    pub params: SessionParams,

    // Sequence numbers
    pub exp_cmd_sn: u32,
    pub max_cmd_sn: u32,
    pub stat_sn: u32,

    // Login tracking
    pub current_stage: u8,
    pub next_stage: u8,

    // Command tracking
    pub pending_writes: HashMap<u32, PendingWrite>,
    pub next_ttt: u32,
    pub last_sense_data: Option<Vec<u8>>,

    // Authentication
    pub auth_config: AuthConfig,
    pub chap_state: Option<ChapAuthState>,
    pub target_chap_state: Option<ChapAuthState>,
    pub chap_completed: bool,
    pub none_auth_negotiated: bool,
    pub allowed_initiators: Option<Vec<String>>,
}

impl Default for SessionData {
    fn default() -> Self {
        SessionData {
            isid: [0u8; 6],
            tsih: 0,
            cid: 0,
            session_type: SessionType::Normal,
            params: SessionParams::default(),
            exp_cmd_sn: 1,
            max_cmd_sn: 1,
            stat_sn: 0,
            current_stage: 0,
            next_stage: 0,
            pending_writes: HashMap::new(),
            next_ttt: 1,
            last_sense_data: None,
            auth_config: AuthConfig::None,
            chap_state: None,
            target_chap_state: None,
            chap_completed: false,
            none_auth_negotiated: false,
            allowed_initiators: None,
        }
    }
}

impl SessionData {
    /// Generate next Target Transfer Tag
    pub fn next_target_transfer_tag(&mut self) -> u32 {
        let ttt = self.next_ttt;
        self.next_ttt = self.next_ttt.wrapping_add(1);
        if self.next_ttt == 0xFFFF_FFFF {
            self.next_ttt = 1;
        }
        ttt
    }

    /// Get next StatSN and increment
    pub fn next_stat_sn(&mut self) -> u32 {
        let sn = self.stat_sn;
        self.stat_sn = self.stat_sn.wrapping_add(1);
        sn
    }

    /// Validate command sequence number
    pub fn validate_cmd_sn(&mut self, cmd_sn: u32) -> bool {
        let in_window = sn_in_window(cmd_sn, self.exp_cmd_sn, self.max_cmd_sn);
        if in_window && cmd_sn == self.exp_cmd_sn {
            self.exp_cmd_sn = self.exp_cmd_sn.wrapping_add(1);
            self.max_cmd_sn = self.max_cmd_sn.wrapping_add(1);
        }
        in_window
    }

    /// Generate TSIH
    fn generate_tsih() -> u16 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        ((duration.as_millis() & 0xFFFF) as u16).max(1)
    }

    /// Apply initiator parameter during negotiation
    pub fn apply_initiator_param(&mut self, key: &str, value: &str) {
        match key {
            "InitiatorName" => self.params.initiator_name = value.to_string(),
            "InitiatorAlias" => self.params.initiator_alias = value.to_string(),
            "TargetName" => self.params.target_name = value.to_string(),
            "SessionType" => {
                match value {
                    "Discovery" => self.session_type = SessionType::Discovery,
                    "Normal" => self.session_type = SessionType::Normal,
                    _ => {
                        log::warn!("Invalid SessionType '{}' - only 'Discovery' and 'Normal' are supported", value);
                        self.params.invalid_session_type = Some(value.to_string());
                    }
                }
            }
            "MaxRecvDataSegmentLength" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.params.max_xmit_data_segment_length = v;
                }
            }
            "MaxBurstLength" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.params.max_burst_length = v.min(self.params.max_burst_length);
                    // RFC 3720 12.14: FirstBurstLength MUST NOT exceed MaxBurstLength.
                    // Re-clamp in case MaxBurstLength is negotiated after FirstBurstLength.
                    self.params.first_burst_length =
                        self.params.first_burst_length.min(self.params.max_burst_length);
                }
            }
            "FirstBurstLength" => {
                if let Ok(v) = value.parse::<u32>() {
                    // RFC 3720 12.14: Minimum result function, and the negotiated
                    // value MUST NOT exceed MaxBurstLength.
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
                // RFC 3720 12.18: boolean result function OR against our value.
                self.params.data_pdu_in_order =
                    self.params.data_pdu_in_order || (value == "Yes");
            }
            "DataSequenceInOrder" => {
                // RFC 3720 12.19: boolean result function OR against our value.
                self.params.data_sequence_in_order =
                    self.params.data_sequence_in_order || (value == "Yes");
            }
            "ErrorRecoveryLevel" => {
                if let Ok(v) = value.parse::<u8>() {
                    self.params.error_recovery_level = v.min(self.params.error_recovery_level);
                }
            }
            "ImmediateData" => {
                self.params.immediate_data = self.params.immediate_data && (value == "Yes");
            }
            "InitialR2T" => {
                self.params.initial_r2t = self.params.initial_r2t || (value == "Yes");
            }
            "HeaderDigest" => {
                self.params.header_digest = crate::session::negotiate_digest(value);
            }
            "DataDigest" => {
                self.params.data_digest = crate::session::negotiate_digest(value);
            }
            // Auth parameters handled separately
            "AuthMethod" | "CHAP_A" | "CHAP_I" | "CHAP_C" | "CHAP_N" | "CHAP_R" => {}
            _ => {
                log::debug!("Ignoring unknown parameter: {}={}", key, value);
            }
        }
    }

    /// Generate target response parameters for login
    pub fn generate_response_params(&self) -> Vec<(String, String)> {
        let mut params = Vec::new();

        if self.session_type == SessionType::Normal && !self.params.target_alias.is_empty() {
            params.push(("TargetAlias".to_string(), self.params.target_alias.clone()));
        }

        params.push(("MaxRecvDataSegmentLength".to_string(), self.params.max_recv_data_segment_length.to_string()));
        params.push(("MaxBurstLength".to_string(), self.params.max_burst_length.to_string()));
        params.push(("FirstBurstLength".to_string(), self.params.first_burst_length.to_string()));
        params.push(("DefaultTime2Wait".to_string(), self.params.default_time2wait.to_string()));
        params.push(("DefaultTime2Retain".to_string(), self.params.default_time2retain.to_string()));
        params.push(("MaxOutstandingR2T".to_string(), self.params.max_outstanding_r2t.to_string()));
        params.push(("DataPDUInOrder".to_string(), if self.params.data_pdu_in_order { "Yes" } else { "No" }.to_string()));
        params.push(("DataSequenceInOrder".to_string(), if self.params.data_sequence_in_order { "Yes" } else { "No" }.to_string()));
        params.push(("ErrorRecoveryLevel".to_string(), self.params.error_recovery_level.to_string()));
        params.push(("ImmediateData".to_string(), if self.params.immediate_data { "Yes" } else { "No" }.to_string()));
        params.push(("InitialR2T".to_string(), if self.params.initial_r2t { "Yes" } else { "No" }.to_string()));
        params.push(("HeaderDigest".to_string(), match self.params.header_digest { DigestType::None => "None", DigestType::CRC32C => "CRC32C" }.to_string()));
        params.push(("DataDigest".to_string(), match self.params.data_digest { DigestType::None => "None", DigestType::CRC32C => "CRC32C" }.to_string()));

        params
    }

    /// Create login reject response
    pub fn create_login_reject(&self, itt: u32, status_class: u8, status_detail: u8) -> IscsiPdu {
        IscsiPdu::login_response(
            self.isid,
            0,
            self.stat_sn,
            self.exp_cmd_sn,
            self.max_cmd_sn,
            status_class,
            status_detail,
            self.current_stage,
            self.next_stage,
            false,
            itt,
            Vec::new(),
        )
    }

    /// Create shutdown reject
    pub fn create_shutdown_reject(&self, itt: u32) -> IscsiPdu {
        log::info!("Rejecting login attempt during graceful shutdown");
        self.create_login_reject(itt, pdu::login_status::TARGET_ERROR, 0x01)
    }

    /// Create out of resources reject
    pub fn create_out_of_resources_reject(&self, itt: u32) -> IscsiPdu {
        log::error!("Rejecting login due to resource exhaustion");
        self.create_login_reject(itt, pdu::login_status::TARGET_ERROR, 0x02)
    }

    /// Create invalid request during login reject
    pub fn create_invalid_request_during_login_reject(&self, itt: u32) -> IscsiPdu {
        log::warn!("Rejecting invalid request during login phase");
        self.create_login_reject(itt, pdu::login_status::INITIATOR_ERROR, 0x0B)
    }

    /// Create authorization failure reject
    pub fn create_authorization_failure_reject(&self, itt: u32) -> IscsiPdu {
        log::warn!("Rejecting login due to ACL check failure");
        self.create_login_reject(itt, pdu::login_status::INITIATOR_ERROR, 0x02)
    }
}

// =============================================================================
// Typestate Session
// =============================================================================

/// iSCSI Session with compile-time state tracking
#[derive(Debug)]
pub struct TypestateSession<S: SessionState> {
    pub data: SessionData,
    _state: PhantomData<S>,
}

// =============================================================================
// Session in Free State
// =============================================================================

impl TypestateSession<Free> {
    /// Create a new session in Free state
    pub fn new() -> Self {
        TypestateSession {
            data: SessionData::default(),
            _state: PhantomData,
        }
    }

    /// Configure authentication
    pub fn with_auth(mut self, auth_config: AuthConfig) -> Self {
        self.data.auth_config = auth_config;
        self
    }

    /// Set target name and alias
    pub fn with_target(mut self, name: &str, alias: &str) -> Self {
        self.data.params.target_name = name.to_string();
        self.data.params.target_alias = alias.to_string();
        self
    }

    /// Configure allowed initiators (ACL)
    pub fn with_allowed_initiators(mut self, initiators: Option<Vec<String>>) -> Self {
        self.data.allowed_initiators = initiators;
        self
    }

    /// Process first login request
    pub fn process_login(
        mut self,
        pdu: &IscsiPdu,
        target_name: &str,
    ) -> ScsiResult<(AnySession, Vec<IscsiPdu>)> {
        let login = pdu.parse_login_request()?;
        let itt = pdu.itt;

        // Version check
        const TARGET_VERSION: u8 = 0x00;
        if TARGET_VERSION < login.version_min || TARGET_VERSION > login.version_max {
            log::warn!("Login rejected: version mismatch");
            let response = self.data.create_login_reject(itt, pdu::login_status::INITIATOR_ERROR, 0x05);
            return Ok((AnySession::Failed(self.into_failed()), vec![response]));
        }

        // Initialize session
        self.data.isid = login.isid;
        self.data.cid = login.cid;
        self.data.exp_cmd_sn = login.cmd_sn;
        self.data.max_cmd_sn = login.cmd_sn + 1;
        self.data.params.target_name = target_name.to_string();
        self.data.current_stage = login.csg;
        self.data.next_stage = login.nsg;

        // Apply parameters
        for (key, value) in &login.parameters {
            self.data.apply_initiator_param(key, value);
        }

        // Validate required parameters
        let has_initiator_name = login.parameters.iter().any(|(k, _)| k == "InitiatorName");
        if !has_initiator_name && self.data.params.initiator_name.is_empty() {
            log::warn!("Login rejected: missing InitiatorName");
            let response = self.data.create_login_reject(itt, pdu::login_status::INITIATOR_ERROR, 0x07);
            return Ok((AnySession::Failed(self.into_failed()), vec![response]));
        }

        // Validate target name for normal sessions
        if self.data.session_type == SessionType::Normal {
            let requested_target = login.parameters.iter()
                .find(|(k, _)| k == "TargetName")
                .map(|(_, v)| v.as_str());

            if let Some(req_name) = requested_target {
                if req_name != target_name {
                    log::warn!("Login rejected: target '{}' not found", req_name);
                    let response = self.data.create_login_reject(itt, pdu::login_status::INITIATOR_ERROR, 0x03);
                    return Ok((AnySession::Failed(self.into_failed()), vec![response]));
                }
            }
        }

        // Check for invalid session type
        if let Some(ref invalid_type) = self.data.params.invalid_session_type {
            log::warn!("Login rejected: unsupported SessionType '{}'", invalid_type);
            let response = self.data.create_login_reject(itt, pdu::login_status::INITIATOR_ERROR, 0x09);
            return Ok((AnySession::Failed(self.into_failed()), vec![response]));
        }

        // Transition to appropriate state based on client's CSG
        match login.csg {
            0 => {
                // Client wants SecurityNegotiation
                let security_session: TypestateSession<SecurityNegotiation> = TypestateSession {
                    data: self.data,
                    _state: PhantomData,
                };
                security_session.continue_login(pdu, target_name)
            }
            1 => {
                // Client wants to skip SecurityNegotiation and go to LoginOperationalNegotiation
                let login_session: TypestateSession<LoginOperationalNegotiation> = TypestateSession {
                    data: self.data,
                    _state: PhantomData,
                };
                login_session.process_login(pdu, target_name)
            }
            _ => {
                // Invalid CSG
                log::warn!("Login rejected: invalid CSG {} in initial login", login.csg);
                let response = self.data.create_login_reject(itt, pdu::login_status::INITIATOR_ERROR, 0x02);
                Ok((AnySession::Failed(self.into_failed()), vec![response]))
            }
        }
    }

    fn into_failed(self) -> TypestateSession<Failed> {
        TypestateSession { data: self.data, _state: PhantomData }
    }
}

impl Default for TypestateSession<Free> {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Session in SecurityNegotiation State
// =============================================================================

impl TypestateSession<SecurityNegotiation> {
    /// Process login during security negotiation
    pub fn process_login(
        mut self,
        pdu: &IscsiPdu,
        target_name: &str,
    ) -> ScsiResult<(AnySession, Vec<IscsiPdu>)> {
        let login = pdu.parse_login_request()?;

        // Apply parameters
        for (key, value) in &login.parameters {
            self.data.apply_initiator_param(key, value);
        }

        self.data.current_stage = login.csg;
        self.data.next_stage = login.nsg;

        self.continue_login(pdu, target_name)
    }

    fn continue_login(
        mut self,
        pdu: &IscsiPdu,
        _target_name: &str,
    ) -> ScsiResult<(AnySession, Vec<IscsiPdu>)> {
        let login = pdu.parse_login_request()?;
        let itt = pdu.itt;

        // Handle CHAP authentication
        let (auth_complete, auth_params) = match self.handle_chap_auth(&login.parameters) {
            Ok(result) => result,
            Err(e) => {
                log::warn!("Login rejected: {}", e);
                let response = self.data.create_login_reject(itt, pdu::login_status::INITIATOR_ERROR, 0x01);
                return Ok((AnySession::Failed(self.into_failed()), vec![response]));
            }
        };

        // If auth in progress, send CHAP params and stay in SecurityNegotiation
        if !auth_params.is_empty() {
            let response_data = serialize_text_parameters(&auth_params);
            self.data.stat_sn = self.data.stat_sn.wrapping_add(1);

            let response = IscsiPdu::login_response(
                self.data.isid, self.data.tsih, self.data.stat_sn,
                self.data.exp_cmd_sn, self.data.max_cmd_sn,
                0, 0, 0, 0, false, itt, response_data,
            );
            return Ok((AnySession::SecurityNegotiation(self), vec![response]));
        }

        if !auth_complete {
            log::warn!("Login rejected: authentication failed");
            let response = self.data.create_login_reject(itt, pdu::login_status::INITIATOR_ERROR, 0x01);
            return Ok((AnySession::Failed(self.into_failed()), vec![response]));
        }

        // Check ACL
        if let Some(ref allowed) = self.data.allowed_initiators {
            if !allowed.contains(&self.data.params.initiator_name) {
                log::warn!("Login rejected: initiator '{}' not in ACL", self.data.params.initiator_name);
                let response = self.data.create_authorization_failure_reject(itt);
                return Ok((AnySession::Failed(self.into_failed()), vec![response]));
            }
        }

        // Handle state transition
        let transit = login.transit && auth_complete;
        if transit {
            match (login.csg, login.nsg) {
                (0, 1) => {
                    // Security -> LoginOperationalNegotiation
                    self.data.stat_sn = self.data.stat_sn.wrapping_add(1);
                    let response = IscsiPdu::login_response(
                        self.data.isid, self.data.tsih, self.data.stat_sn,
                        self.data.exp_cmd_sn, self.data.max_cmd_sn,
                        0, 0, login.csg, login.nsg, true, itt, vec![],
                    );
                    let new_session: TypestateSession<LoginOperationalNegotiation> = TypestateSession {
                        data: self.data, _state: PhantomData,
                    };
                    Ok((AnySession::LoginOperationalNegotiation(new_session), vec![response]))
                }
                (0, 3) => {
                    // Security -> FullFeaturePhase (direct)
                    if self.data.session_type == SessionType::Normal {
                        self.data.tsih = SessionData::generate_tsih();
                    }
                    self.data.stat_sn = self.data.stat_sn.wrapping_add(1);

                    let response_params = if self.data.session_type == SessionType::Discovery {
                        vec![]
                    } else {
                        self.data.generate_response_params()
                    };
                    let response_data = serialize_text_parameters(&response_params);

                    let response = IscsiPdu::login_response(
                        self.data.isid, self.data.tsih, self.data.stat_sn,
                        self.data.exp_cmd_sn, self.data.max_cmd_sn,
                        0, 0, login.csg, login.nsg, true, itt, response_data,
                    );
                    let new_session: TypestateSession<FullFeaturePhase> = TypestateSession {
                        data: self.data, _state: PhantomData,
                    };
                    Ok((AnySession::FullFeaturePhase(new_session), vec![response]))
                }
                _ => {
                    // Invalid state transition - reject with our current stage
                    self.data.stat_sn = self.data.stat_sn.wrapping_add(1);
                    let response = IscsiPdu::login_response(
                        self.data.isid, self.data.tsih, self.data.stat_sn,
                        self.data.exp_cmd_sn, self.data.max_cmd_sn,
                        0, 0, 0, 1, false, itt, vec![],  // CSG=0 (SecurityNegotiation), NSG=1
                    );
                    Ok((AnySession::SecurityNegotiation(self), vec![response]))
                }
            }
        } else {
            // No transit - report our current stage
            self.data.stat_sn = self.data.stat_sn.wrapping_add(1);
            let response = IscsiPdu::login_response(
                self.data.isid, self.data.tsih, self.data.stat_sn,
                self.data.exp_cmd_sn, self.data.max_cmd_sn,
                0, 0, 0, 0, false, itt, vec![],  // CSG=0, NSG=0 (stay in SecurityNegotiation)
            );
            Ok((AnySession::SecurityNegotiation(self), vec![response]))
        }
    }

    fn handle_chap_auth(&mut self, params: &[(String, String)]) -> ScsiResult<(bool, Vec<(String, String)>)> {
        let auth_method = params.iter().find(|(k, _)| k == "AuthMethod").map(|(_, v)| v.as_str());
        let supports_chap = auth_method.map(|m| m.contains("CHAP")).unwrap_or(false);

        match &self.data.auth_config {
            AuthConfig::None => {
                // If we've already negotiated None auth, don't send the response again
                if self.data.none_auth_negotiated {
                    return Ok((true, vec![]));
                }

                if auth_method.is_some() {
                    // Mark that we've negotiated None auth
                    self.data.none_auth_negotiated = true;
                    Ok((true, vec![("AuthMethod".to_string(), "None".to_string())]))
                } else {
                    Ok((true, vec![]))
                }
            }
            AuthConfig::Chap { credentials } | AuthConfig::MutualChap { target_credentials: credentials, .. } => {
                if self.data.chap_completed && params.is_empty() {
                    return Ok((true, vec![]));
                }

                let chap_a = params.iter().find(|(k, _)| k == "CHAP_A").map(|(_, v)| v.as_str());
                let chap_in_progress = self.data.chap_state.is_some() || chap_a.is_some();

                if supports_chap || chap_in_progress {
                    if chap_a.is_none() && self.data.chap_state.is_none() {
                        Ok((false, vec![
                            ("TargetPortalGroupTag".to_string(), "1".to_string()),
                            ("AuthMethod".to_string(), "CHAP".to_string()),
                        ]))
                    } else if chap_a.is_some() && self.data.chap_state.is_none() {
                        let chap_state = ChapAuthState::new(false);
                        let result = vec![
                            ("CHAP_A".to_string(), "5".to_string()),
                            ("CHAP_I".to_string(), chap_state.identifier_str()),
                            ("CHAP_C".to_string(), chap_state.challenge_hex()),
                        ];
                        self.data.chap_state = Some(chap_state);
                        Ok((false, result))
                    } else if self.data.chap_state.is_some() {
                        let chap_n = params.iter().find(|(k, _)| k == "CHAP_N").map(|(_, v)| v.as_str());
                        let chap_r = params.iter().find(|(k, _)| k == "CHAP_R").map(|(_, v)| v.as_str());

                        if let (Some(username), Some(response_hex)) = (chap_n, chap_r) {
                            if username != credentials.username {
                                return Err(IscsiError::Auth(format!("Unknown user '{}'", username)));
                            }

                            let response = parse_chap_response(response_hex)?;
                            let chap_state = self.data.chap_state.as_ref().unwrap();

                            if chap_state.validate_response(&response, &credentials.secret) {
                                log::info!("CHAP authentication successful for '{}'", username);

                                // Handle mutual CHAP
                                if let AuthConfig::MutualChap { initiator_credentials, .. } = &self.data.auth_config {
                                    let target_chap_i = params.iter().find(|(k, _)| k == "CHAP_I").map(|(_, v)| v.as_str());
                                    let target_chap_c = params.iter().find(|(k, _)| k == "CHAP_C").map(|(_, v)| v.as_str());

                                    if let (Some(chap_i), Some(chap_c_hex)) = (target_chap_i, target_chap_c) {
                                        let identifier = chap_i.parse::<u8>().map_err(|e| IscsiError::Auth(format!("Invalid CHAP_I: {}", e)))?;
                                        let chap_c_clean = chap_c_hex.strip_prefix("0x").unwrap_or(chap_c_hex);
                                        let challenge = hex::decode(chap_c_clean).map_err(|e| IscsiError::Auth(format!("Invalid CHAP_C: {}", e)))?;

                                        let mut data = Vec::new();
                                        data.push(identifier);
                                        data.extend_from_slice(initiator_credentials.secret.as_bytes());
                                        data.extend_from_slice(&challenge);
                                        let target_response = md5::compute(&data).0.to_vec();
                                        let response_hex = format!("0x{}", target_response.iter().map(|b| format!("{:02x}", b)).collect::<String>());

                                        self.data.chap_state = None;
                                        self.data.chap_completed = true;
                                        return Ok((true, vec![
                                            ("CHAP_N".to_string(), initiator_credentials.username.clone()),
                                            ("CHAP_R".to_string(), response_hex),
                                        ]));
                                    }
                                }

                                self.data.chap_state = None;
                                self.data.chap_completed = true;
                                Ok((true, vec![]))
                            } else {
                                Err(IscsiError::Auth(format!("Invalid password for user '{}'", username)))
                            }
                        } else {
                            Err(IscsiError::Auth("Missing CHAP_N or CHAP_R".to_string()))
                        }
                    } else {
                        Err(IscsiError::Auth("CHAP protocol error".to_string()))
                    }
                } else {
                    Err(IscsiError::Auth("CHAP required but not requested".to_string()))
                }
            }
        }
    }

    fn into_failed(self) -> TypestateSession<Failed> {
        TypestateSession { data: self.data, _state: PhantomData }
    }
}

// =============================================================================
// Session in LoginOperationalNegotiation State
// =============================================================================

impl TypestateSession<LoginOperationalNegotiation> {
    /// Process login during operational negotiation
    pub fn process_login(
        mut self,
        pdu: &IscsiPdu,
        _target_name: &str,
    ) -> ScsiResult<(AnySession, Vec<IscsiPdu>)> {
        let login = pdu.parse_login_request()?;
        let itt = pdu.itt;

        for (key, value) in &login.parameters {
            self.data.apply_initiator_param(key, value);
        }

        self.data.current_stage = login.csg;
        self.data.next_stage = login.nsg;

        if login.transit && login.nsg == 3 {
            // Transition to FullFeaturePhase
            if self.data.session_type == SessionType::Normal {
                self.data.tsih = SessionData::generate_tsih();
            }
            self.data.stat_sn = self.data.stat_sn.wrapping_add(1);

            let response_params = self.data.generate_response_params();
            let response_data = serialize_text_parameters(&response_params);

            let response = IscsiPdu::login_response(
                self.data.isid, self.data.tsih, self.data.stat_sn,
                self.data.exp_cmd_sn, self.data.max_cmd_sn,
                0, 0, login.csg, login.nsg, true, itt, response_data,
            );

            let new_session: TypestateSession<FullFeaturePhase> = TypestateSession {
                data: self.data, _state: PhantomData,
            };
            Ok((AnySession::FullFeaturePhase(new_session), vec![response]))
        } else {
            // No transit - report our current stage
            self.data.stat_sn = self.data.stat_sn.wrapping_add(1);
            let response = IscsiPdu::login_response(
                self.data.isid, self.data.tsih, self.data.stat_sn,
                self.data.exp_cmd_sn, self.data.max_cmd_sn,
                0, 0, 1, 1, false, itt, vec![],  // CSG=1, NSG=1 (stay in LoginOperationalNegotiation)
            );
            Ok((AnySession::LoginOperationalNegotiation(self), vec![response]))
        }
    }
}

// =============================================================================
// Session in FullFeaturePhase State
// =============================================================================

impl TypestateSession<FullFeaturePhase> {
    /// Check if this is a discovery session
    pub fn is_discovery(&self) -> bool {
        self.data.session_type == SessionType::Discovery
    }

    /// Process NOP-Out (ping) request
    pub fn process_nop_out(&mut self, pdu: &IscsiPdu) -> ScsiResult<IscsiPdu> {
        let nop = pdu.parse_nop_out()?;

        if nop.itt == 0xFFFF_FFFF {
            return Err(IscsiError::Protocol("Unsolicited NOP-Out response".to_string()));
        }

        Ok(IscsiPdu::nop_in(
            nop.itt, 0xFFFF_FFFF,
            self.data.next_stat_sn(),
            self.data.exp_cmd_sn, self.data.max_cmd_sn,
            nop.lun,
        ))
    }

    /// Process logout request - transitions to Logout state
    pub fn process_logout(mut self, pdu: &IscsiPdu) -> ScsiResult<(TypestateSession<Logout>, IscsiPdu)> {
        let logout = pdu.parse_logout_request()?;

        let response = IscsiPdu::logout_response(
            logout.itt,
            self.data.next_stat_sn(),
            self.data.exp_cmd_sn,
            self.data.max_cmd_sn,
            pdu::logout_response::SUCCESS,
            self.data.params.default_time2wait,
            self.data.params.default_time2retain,
        );

        Ok((TypestateSession { data: self.data, _state: PhantomData }, response))
    }

    /// Handle SendTargets discovery request
    pub fn handle_send_targets(&self, target_name: &str, target_address: &str) -> Vec<(String, String)> {
        vec![
            ("TargetName".to_string(), target_name.to_string()),
            ("TargetAddress".to_string(), format!("{},1", target_address)),
        ]
    }

    /// Transition to Failed state
    pub fn fail(self) -> TypestateSession<Failed> {
        TypestateSession { data: self.data, _state: PhantomData }
    }
}

// =============================================================================
// Terminal States
// =============================================================================

impl TypestateSession<Logout> {
    pub fn is_complete(&self) -> bool { true }
}

impl TypestateSession<Failed> {
    pub fn is_failed(&self) -> bool { true }
}

// =============================================================================
// AnySession - Runtime dispatch wrapper
// =============================================================================

/// A wrapper enum for runtime dispatch when storing sessions
pub enum AnySession {
    Free(TypestateSession<Free>),
    SecurityNegotiation(TypestateSession<SecurityNegotiation>),
    LoginOperationalNegotiation(TypestateSession<LoginOperationalNegotiation>),
    FullFeaturePhase(TypestateSession<FullFeaturePhase>),
    Logout(TypestateSession<Logout>),
    Failed(TypestateSession<Failed>),
}

impl AnySession {
    /// Create a new session
    pub fn new() -> Self {
        AnySession::Free(TypestateSession::new())
    }

    /// Create with configuration
    pub fn new_configured(auth_config: AuthConfig, target_name: &str, target_alias: &str, allowed_initiators: Option<Vec<String>>) -> Self {
        let session = TypestateSession::new()
            .with_auth(auth_config)
            .with_target(target_name, target_alias)
            .with_allowed_initiators(allowed_initiators);
        AnySession::Free(session)
    }

    /// Check if session is in full feature phase
    pub fn is_full_feature(&self) -> bool {
        matches!(self, AnySession::FullFeaturePhase(_))
    }

    /// Check if session is in a login phase
    pub fn is_login_phase(&self) -> bool {
        matches!(self, AnySession::Free(_) | AnySession::SecurityNegotiation(_) | AnySession::LoginOperationalNegotiation(_))
    }

    /// Check if session has ended
    pub fn is_ended(&self) -> bool {
        matches!(self, AnySession::Logout(_) | AnySession::Failed(_))
    }

    /// Get session data reference (if available)
    pub fn data(&self) -> Option<&SessionData> {
        match self {
            AnySession::Free(s) => Some(&s.data),
            AnySession::SecurityNegotiation(s) => Some(&s.data),
            AnySession::LoginOperationalNegotiation(s) => Some(&s.data),
            AnySession::FullFeaturePhase(s) => Some(&s.data),
            AnySession::Logout(s) => Some(&s.data),
            AnySession::Failed(s) => Some(&s.data),
        }
    }

    /// Get mutable session data reference for FullFeaturePhase
    pub fn data_mut(&mut self) -> Option<&mut SessionData> {
        match self {
            AnySession::FullFeaturePhase(s) => Some(&mut s.data),
            _ => None,
        }
    }

    /// Process login PDU (dispatches based on current state)
    pub fn process_login(self, pdu: &IscsiPdu, target_name: &str) -> ScsiResult<(AnySession, Vec<IscsiPdu>)> {
        match self {
            AnySession::Free(s) => s.process_login(pdu, target_name),
            AnySession::SecurityNegotiation(s) => s.process_login(pdu, target_name),
            AnySession::LoginOperationalNegotiation(s) => s.process_login(pdu, target_name),
            _ => Err(IscsiError::Protocol("Cannot process login in current state".to_string())),
        }
    }

    /// Process NOP-Out (only in FullFeaturePhase)
    pub fn process_nop_out(&mut self, pdu: &IscsiPdu) -> ScsiResult<IscsiPdu> {
        match self {
            AnySession::FullFeaturePhase(s) => s.process_nop_out(pdu),
            _ => Err(IscsiError::Protocol("NOP-Out only valid in FullFeaturePhase".to_string())),
        }
    }

    /// Process logout (only in FullFeaturePhase, consumes and transitions)
    pub fn process_logout(self, pdu: &IscsiPdu) -> ScsiResult<(AnySession, IscsiPdu)> {
        match self {
            AnySession::FullFeaturePhase(s) => {
                let (logout_session, response) = s.process_logout(pdu)?;
                Ok((AnySession::Logout(logout_session), response))
            }
            _ => Err(IscsiError::Protocol("Logout only valid in FullFeaturePhase".to_string())),
        }
    }

    /// Get state name for logging
    pub fn state_name(&self) -> &'static str {
        match self {
            AnySession::Free(_) => "Free",
            AnySession::SecurityNegotiation(_) => "SecurityNegotiation",
            AnySession::LoginOperationalNegotiation(_) => "LoginOperationalNegotiation",
            AnySession::FullFeaturePhase(_) => "FullFeaturePhase",
            AnySession::Logout(_) => "Logout",
            AnySession::Failed(_) => "Failed",
        }
    }
}

impl Default for AnySession {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn sn_in_window(sn: u32, exp_sn: u32, max_sn: u32) -> bool {
    let diff_exp = sn.wrapping_sub(exp_sn) as i32;
    let diff_max = max_sn.wrapping_sub(sn) as i32;
    diff_exp >= 0 && diff_max >= 0
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typestate_creation() {
        let session: TypestateSession<Free> = TypestateSession::new();
        assert_eq!(session.data.tsih, 0);
    }

    #[test]
    fn test_any_session_wrapper() {
        let any_session = AnySession::new();
        assert!(!any_session.is_full_feature());
        assert!(any_session.is_login_phase());
        assert!(!any_session.is_ended());
        assert_eq!(any_session.state_name(), "Free");
    }

    #[test]
    fn test_session_data_defaults() {
        let data = SessionData::default();
        assert_eq!(data.exp_cmd_sn, 1);
        assert_eq!(data.max_cmd_sn, 1);
        assert_eq!(data.stat_sn, 0);
        assert_eq!(data.next_ttt, 1);
    }

    #[test]
    fn test_ttt_generation() {
        let mut data = SessionData::default();
        assert_eq!(data.next_target_transfer_tag(), 1);
        assert_eq!(data.next_target_transfer_tag(), 2);
        assert_eq!(data.next_target_transfer_tag(), 3);
    }

    #[test]
    fn test_parameter_application() {
        let mut data = SessionData::default();
        data.apply_initiator_param("InitiatorName", "iqn.2025.test:initiator");
        assert_eq!(data.params.initiator_name, "iqn.2025.test:initiator");

        data.apply_initiator_param("SessionType", "Discovery");
        assert_eq!(data.session_type, SessionType::Discovery);
    }
}
