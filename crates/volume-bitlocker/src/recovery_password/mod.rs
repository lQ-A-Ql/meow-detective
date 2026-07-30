//! Recovery of a numerical recovery password from an authenticated VMK.

mod error;
mod formatter;
mod material;
mod protector;
mod provenance;
mod recover;
mod reverse_datum;

pub use error::RecoveryPasswordRecoveryError;
pub use protector::{recovery_password_protectors, RecoveryPasswordProtectorIdentity};
pub use provenance::{RecoveredRecoveryPassword, RecoveryPasswordProvenance};
pub use recover::recover_recovery_password;

#[cfg(test)]
#[path = "../../tests/unit/recovery_password/mod.rs"]
mod tests;
