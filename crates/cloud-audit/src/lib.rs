pub mod aws;
pub mod azure;
pub mod gcp;
pub mod m365;
pub mod normalize;

pub use aws::{parse_cloudtrail, AwsCloudTrailRecord};
pub use azure::{parse_azure_activity_log, AzureActivityLogRecord};
pub use gcp::{parse_gcp_audit_log, GcpAuditLogRecord};
pub use m365::parse_m365_audit_log;
pub use normalize::{CloudAuditEntry, CloudAuditSource};
