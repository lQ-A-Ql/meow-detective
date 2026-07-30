use thiserror::Error;

/// A credential-free failure from VMK-to-recovery-password reconstruction.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RecoveryPasswordRecoveryError {
    #[error("the selected recovery-password protector is not present")]
    ProtectorNotFound,

    #[error("the selected recovery-password protector is duplicated")]
    AmbiguousProtector,

    #[error("recovery-password protector metadata is malformed: {reason}")]
    MalformedProtector { reason: &'static str },

    #[error("the recovered VMK did not authenticate the recovery-password datum")]
    AuthenticationFailed,

    #[error("authenticated recovery-password material is malformed: {reason}")]
    InvalidMaterial { reason: &'static str },
}
