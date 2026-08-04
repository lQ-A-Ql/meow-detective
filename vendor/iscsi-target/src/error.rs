//! Error types for iSCSI target operations

use thiserror::Error;

/// iSCSI target errors
#[derive(Debug, Error)]
pub enum IscsiError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("SCSI error: {0}")]
    Scsi(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Invalid PDU: {0}")]
    InvalidPdu(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authentication error: {0}")]
    Auth(String),
}

/// Result type for SCSI operations
pub type ScsiResult<T> = Result<T, IscsiError>;

/// Decode iSCSI login status codes into helpful error messages
pub fn decode_login_status(status_class: u8, status_detail: u8) -> String {
    let status_code = ((status_class as u16) << 8) | (status_detail as u16);

    match status_code {
        // Success
        0x0000 => "Login successful".to_string(),

        // Redirection (0x01xx)
        0x0101 => {
            "Target moved temporarily\n\
             \n\
             The target has temporarily moved to a different portal.\n\
             Check the login response for the new portal address."
                .to_string()
        }
        0x0102 => {
            "Target moved permanently\n\
             \n\
             The target has permanently moved to a different portal.\n\
             Update your configuration with the new portal address."
                .to_string()
        }

        // Initiator Error (0x02xx)
        0x0200 => "Authentication failure (initiator error)".to_string(),
        0x0201 => {
            "Authentication failed\n\
             \n\
             The target rejected your authentication credentials.\n\
             \n\
             Troubleshooting:\n\
             1. Verify username/password in configuration\n\
             2. Check if target requires CHAP authentication\n\
             3. Verify authentication method (None, CHAP, etc.)"
                .to_string()
        }
        0x0202 => {
            "Authorization failure\n\
             \n\
             Authentication succeeded but you're not authorized to access this target.\n\
             \n\
             Troubleshooting:\n\
             1. Check target's ACL (Access Control List)\n\
             2. For TGTD:\n\
                sudo tgtadm --lld iscsi --op bind --mode target --tid 1 -I <initiator-iqn>\n\
             3. Verify initiator IQN is allowed by target"
                .to_string()
        }
        0x0203 => {
            "Target not found\n\
             \n\
             The target IQN doesn't exist on this portal.\n\
             \n\
             Troubleshooting:\n\
             1. Run discovery to see available targets:\n\
                cargo run --example discover_targets -- <portal>\n\
             2. Verify target IQN in configuration\n\
             3. Check if target is running and properly configured"
                .to_string()
        }
        0x0204 => "Target removed - target has been removed from service".to_string(),
        0x0205 => {
            "Unsupported iSCSI version - RFC 3720: UNSUPPORTED_VERSION (0x0205)\n\
             \n\
             The initiator's protocol version is not supported by this target.\n\
             \n\
             Details:\n\
             - Target supports: iSCSI version 0x00 (RFC 3720)\n\
             - Your initiator requested a version outside target's supported range\n\
             \n\
             Troubleshooting:\n\
             1. Check initiator iSCSI version (should be 0x00 for RFC 3720)\n\
             2. Ensure initiator firmware is up to date\n\
             3. Check for non-standard iSCSI implementations\n\
             4. Review Version-min and Version-max in login request"
                .to_string()
        }
        0x0206 => {
            "Too many connections\n\
             \n\
             Target has reached maximum number of connections.\n\
             \n\
             Troubleshooting:\n\
             1. Close existing connections\n\
             2. Check target's MaxConnections parameter\n\
             3. Wait and retry"
                .to_string()
        }
        0x0207 => {
            "Missing required parameter\n\
             \n\
             Login request is missing a required parameter.\n\
             \n\
             Common missing parameters:\n\
             - InitiatorName (always required)\n\
             - TargetName (required for normal sessions)\n\
             - SessionType (Discovery or Normal)"
                .to_string()
        }
        0x0208 => "Cannot include connection in session".to_string(),
        0x0209 => {
            "Session type not supported\n\
             \n\
             Target doesn't support the requested session type.\n\
             \n\
             Troubleshooting:\n\
             - If doing discovery: target may not support SendTargets\n\
             - If normal session: verify TargetName is correct"
                .to_string()
        }
        0x020A => "Session does not exist".to_string(),
        0x020B => {
            "Invalid request during login - RFC 3720: INVALID_REQUEST_DURING_LOGIN (0x020B)\n\
             \n\
             The request sent during login phase was invalid or not allowed.\n\
             \n\
             Common causes:\n\
             1. Sending non-login PDU (e.g., SCSI command) before login completes\n\
             2. Invalid stage transition requested\n\
             3. Text request sent when not in text negotiation stage\n\
             4. Multiple simultaneous login requests\n\
             \n\
             Troubleshooting:\n\
             - Ensure only Login Request PDUs are sent during login phase\n\
             - Complete login negotiation before sending other commands\n\
             - Check CurrentStage and NextStage values in login requests\n\
             - Review RFC 3720 Section 5.3 for valid login phase sequences"
                .to_string()
        }

        // Target Error (0x03xx)
        0x0300 => "Target error (unspecified)".to_string(),
        0x0301 => {
            "Target service unavailable\n\
             \n\
             The target is temporarily unable to service requests.\n\
             \n\
             Troubleshooting:\n\
             1. Wait and retry\n\
             2. Check target logs for errors\n\
             3. Verify target has sufficient resources"
                .to_string()
        }
        0x0302 => {
            "Target out of resources - RFC 3720: OUT_OF_RESOURCES (0x0302)\n\
             \n\
             The target cannot process the login request due to resource exhaustion.\n\
             \n\
             Common causes:\n\
             1. Memory allocation failure\n\
             2. Maximum session limit reached\n\
             3. Disk/storage resources exhausted\n\
             4. Internal buffer shortage\n\
             \n\
             Troubleshooting:\n\
             - Wait and retry the connection\n\
             - Check target resource usage (memory, sessions)\n\
             - Close unused sessions to free resources\n\
             - Contact administrator if issue persists\n\
             - This is a temporary condition - target should recover"
                .to_string()
        }

        // Unknown status code
        _ => {
            format!(
                "Unknown login status: class=0x{:02x}, detail=0x{:02x}\n\
                 \n\
                 This is an unrecognized iSCSI login status code.\n\
                 Check RFC 3720 Section 10.13 for status code meanings.",
                status_class, status_detail
            )
        }
    }
}
